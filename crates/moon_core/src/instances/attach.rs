//! This module attaches to a shell another moon holds as if it were this one's: everything it
//! prints, and every key typed into it - what a window's own terminal pane gets from its shell,
//! carried over the moon's socket. `moon tui` attaches to an agent a window started this way, and
//! draws it live in a pane where it can be typed into, menus, arrows and all.
//!
//! The connection starts as every ask does - one line, [`Ask::Attach`], answered by one line,
//! [`Answer::Attached`] - and then carries frames both ways until either end hangs up: a byte
//! saying what the frame is, four saying how long it is, and that many bytes.
//!
//! The shell has one size, the pty's. Whoever attaches sets it to their own while they are
//! attached, as the last client of a tmux session does, and the size it had is put back when
//! they let go - so the window it came from is drawn right again.

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};

use super::{ANSWER_TIMEOUT, Answer, Ask, reach};

/// Tags what the holder sends: what the shell printed.
const PRINTED: u8 = 0;
/// Tags what the holder sends: the shell has ended. Nothing follows it.
const ENDED: u8 = 1;
/// Tags what the attached end sends: keys somebody typed.
const TYPED: u8 = 0;
/// Tags what the attached end sends: the terminal answering a question the program asked it.
const REPLIED: u8 = 1;
/// Tags what the attached end sends: its size, two big-endian `u16`s, columns then rows.
const RESIZED: u8 = 2;

/// Caps the longest frame read: a shell prints in chunks far smaller, and anything bigger is a
/// connection that is not speaking this.
const LONGEST_FRAME: u32 = 16 * 1024 * 1024;
/// Sets how often the holder looks to see whether the shell has ended, while it prints nothing.
const LOOKS_FOR_THE_END_EVERY: Duration = Duration::from_millis(300);

/// This is a shell, as the moon holding it hands it to whoever attaches.
pub struct HeldShell {
    /// Carries what it has printed so far, then everything it prints from here on.
    pub output: mpsc::Receiver<Vec<u8>>,
    /// Is where keys and the rest go.
    pub tty: Arc<dyn ShellTty>,
}

/// Gives the way into a shell: keys, answers to its questions, its size.
pub trait ShellTty: Send + Sync {
    /// Sends keys somebody typed.
    fn write(&self, bytes: &[u8]) -> Result<()>;
    /// Sends the terminal's answer to the program's own question, which is not somebody typing.
    fn reply(&self, bytes: &[u8]) -> Result<()>;
    fn resize(&self, cols: u16, rows: u16) -> Result<()>;
    /// Returns its size now, which is put back once whoever attached lets go - `None` when it
    /// cannot be read, and then nothing is put back.
    fn size(&self) -> Option<(u16, u16)>;
    fn has_exited(&self) -> bool;
}

impl ShellTty for crate::terminal::TerminalSession {
    fn write(&self, bytes: &[u8]) -> Result<()> {
        self.write_input(bytes)
    }

    fn reply(&self, bytes: &[u8]) -> Result<()> {
        self.write_reply(bytes)
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        crate::terminal::TerminalSession::resize(self, cols, rows)
    }

    fn size(&self) -> Option<(u16, u16)> {
        let size = self.master.lock().ok()?.get_size().ok()?;
        Some((size.cols, size.rows))
    }

    fn has_exited(&self) -> bool {
        crate::terminal::TerminalSession::has_exited(self)
    }
}

/// Holds one of this moon's shells for another to attach to: its output, and a way back in.
pub fn hold_a_shell_of(
    terminals: &crate::terminal::TerminalRegistry,
    terminal_id: &str,
) -> Result<HeldShell> {
    let (output, session) = terminals.attach(terminal_id)?;
    Ok(HeldShell {
        output,
        tty: session,
    })
}

/// Serves the holder's side: says the shell is attached, then carries it over the connection
/// on threads of its own until the other end lets go, which returns at once - the thread that
/// answers asks goes on to the next.
pub fn serve_the_attachment(
    stream: UnixStream,
    shell: HeldShell,
    cols: u16,
    rows: u16,
) -> Result<()> {
    // The ask was read with a deadline; what follows comes whenever somebody types.
    stream.set_read_timeout(None)?;
    stream.set_write_timeout(None)?;
    super::write_answer(&stream, &Answer::Attached)?;

    let size_before = shell.tty.size();
    let _ = shell.tty.resize(cols, rows);
    let let_go = Arc::new(AtomicBool::new(false));

    let writer = stream.try_clone()?;
    let (tty, stopped) = (Arc::clone(&shell.tty), Arc::clone(&let_go));
    let output = shell.output;
    thread::Builder::new()
        .name("moon-attached-output".to_string())
        .spawn(move || send_what_the_shell_prints(writer, &output, tty.as_ref(), &stopped))?;

    let tty = shell.tty;
    thread::Builder::new()
        .name("moon-attached-input".to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stream);
            while let Ok((kind, bytes)) = read_frame(&mut reader) {
                let done = match kind {
                    TYPED => tty.write(&bytes),
                    REPLIED => tty.reply(&bytes),
                    RESIZED => match size_in(&bytes) {
                        Some((cols, rows)) => tty.resize(cols, rows),
                        None => Ok(()),
                    },
                    _ => Ok(()),
                };
                if done.is_err() {
                    break;
                }
            }
            let_go.store(true, Ordering::Relaxed);
            if let Some((cols, rows)) = size_before {
                let _ = tty.resize(cols, rows);
            }
        })?;
    Ok(())
}

fn send_what_the_shell_prints(
    mut writer: UnixStream,
    output: &mpsc::Receiver<Vec<u8>>,
    tty: &dyn ShellTty,
    let_go: &AtomicBool,
) {
    while !let_go.load(Ordering::Relaxed) {
        match output.recv_timeout(LOOKS_FOR_THE_END_EVERY) {
            Ok(bytes) => {
                if write_frame(&mut writer, PRINTED, &bytes).is_err() {
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) if !tty.has_exited() => {}
            Err(_) => break,
        }
    }
    let _ = write_frame(&mut writer, ENDED, &[]);
}

/// This is a shell another moon holds, attached to from this one: the way into it. What it
/// prints comes apart, from [`attach`].
pub struct AttachedShell {
    writer: Mutex<UnixStream>,
    ended: Arc<AtomicBool>,
}

impl AttachedShell {
    pub fn write_input(&self, bytes: &[u8]) -> Result<()> {
        self.send(TYPED, bytes)
    }

    pub fn write_reply(&self, bytes: &[u8]) -> Result<()> {
        self.send(REPLIED, bytes)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let mut size = Vec::with_capacity(4);
        size.extend_from_slice(&cols.to_be_bytes());
        size.extend_from_slice(&rows.to_be_bytes());
        self.send(RESIZED, &size)
    }

    /// Says whether the shell ended, or the moon holding it went away.
    pub fn has_exited(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    fn send(&self, kind: u8, bytes: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().expect("the attached shell's writer");
        write_frame(&mut *writer, kind, bytes)
    }
}

impl Drop for AttachedShell {
    /// Lets go: the holder puts the shell's size back when the connection closes.
    fn drop(&mut self) {
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Attaches to a shell the moon running as `moon` holds, at this size: returns what it has
/// printed so far and everything it prints - closed when it ends or the connection does - and
/// the way into it.
pub fn attach(
    moon: u32,
    terminal_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(mpsc::Receiver<Vec<u8>>, AttachedShell)> {
    let stream = reach(moon)?;
    stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
    stream.set_write_timeout(Some(ANSWER_TIMEOUT))?;
    let ask = Ask::Attach {
        terminal_id: terminal_id.to_string(),
        cols,
        rows,
    };
    let mut writing = &stream;
    writeln!(writing, "{}", serde_json::to_string(&ask)?)?;
    writing.flush()?;

    // The same reader goes on to read the frames: whatever it read past the answer's line is
    // the start of them.
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut answer = String::new();
    reader
        .read_line(&mut answer)
        .context("the moon said nothing")?;
    match serde_json::from_str(answer.trim())
        .with_context(|| format!("the moon answered with {answer:?}"))?
    {
        Answer::Attached => {}
        Answer::Refused { reason } => bail!("{reason}"),
        other => bail!("the moon answered {other:?} rather than attaching"),
    }
    stream.set_read_timeout(None)?;
    stream.set_write_timeout(None)?;

    let (sender, output) = mpsc::channel();
    let ended = Arc::new(AtomicBool::new(false));
    let ended_by_the_reader = Arc::clone(&ended);
    thread::Builder::new()
        .name("moon-attached-shell".to_string())
        .spawn(move || {
            while let Ok((kind, bytes)) = read_frame(&mut reader) {
                if kind == ENDED || (kind == PRINTED && sender.send(bytes).is_err()) {
                    break;
                }
            }
            ended_by_the_reader.store(true, Ordering::Relaxed);
        })?;
    Ok((
        output,
        AttachedShell {
            writer: Mutex::new(stream),
            ended,
        },
    ))
}

fn write_frame(writer: &mut impl Write, kind: u8, bytes: &[u8]) -> Result<()> {
    let length = u32::try_from(bytes.len()).context("a frame too long to send")?;
    let mut frame = Vec::with_capacity(5 + bytes.len());
    frame.push(kind);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(bytes);
    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}

fn read_frame(reader: &mut impl Read) -> Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    reader.read_exact(&mut head)?;
    let length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]);
    if length > LONGEST_FRAME {
        bail!("a frame of {length} bytes is not one a shell sends");
    }
    let mut bytes = vec![0u8; length as usize];
    reader.read_exact(&mut bytes)?;
    Ok((head[0], bytes))
}

fn size_in(bytes: &[u8]) -> Option<(u16, u16)> {
    let [c0, c1, r0, r1] = bytes else {
        return None;
    };
    Some((u16::from_be_bytes([*c0, *c1]), u16::from_be_bytes([*r0, *r1])))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fakes a shell that prints what a test hands it and keeps what it was sent.
    #[derive(Default)]
    struct Recorder {
        typed: Mutex<Vec<u8>>,
        sizes: Mutex<Vec<(u16, u16)>>,
        ended: AtomicBool,
    }

    impl ShellTty for Recorder {
        fn write(&self, bytes: &[u8]) -> Result<()> {
            self.typed.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
        fn reply(&self, _bytes: &[u8]) -> Result<()> {
            Ok(())
        }
        fn resize(&self, cols: u16, rows: u16) -> Result<()> {
            self.sizes.lock().unwrap().push((cols, rows));
            Ok(())
        }
        fn size(&self) -> Option<(u16, u16)> {
            Some((120, 40))
        }
        fn has_exited(&self) -> bool {
            self.ended.load(Ordering::Relaxed)
        }
    }

    fn wait_until(what: impl Fn() -> bool) {
        for _ in 0..200 {
            if what() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("it never happened");
    }

    #[test]
    fn what_the_shell_prints_comes_through_and_keys_go_back_at_the_attached_size() {
        let (holder, attached) = UnixStream::pair().unwrap();
        let (prints, output) = mpsc::channel();
        let shell = Arc::new(Recorder::default());
        prints.send(b"what it printed before\r\n".to_vec()).unwrap();
        serve_the_attachment(
            holder,
            HeldShell {
                output,
                tty: shell.clone(),
            },
            80,
            24,
        )
        .unwrap();

        // The attached end reads from after the ask: the answer, then frames.
        let mut reader = BufReader::new(attached.try_clone().unwrap());
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        assert_eq!(
            serde_json::from_str::<Answer>(answer.trim()).unwrap(),
            Answer::Attached
        );
        let (kind, bytes) = read_frame(&mut reader).unwrap();
        assert_eq!((kind, bytes.as_slice()), (PRINTED, &b"what it printed before\r\n"[..]));
        prints.send(b"more".to_vec()).unwrap();
        assert_eq!(read_frame(&mut reader).unwrap().1, b"more");

        let mut writer = attached.try_clone().unwrap();
        write_frame(&mut writer, TYPED, b"\x1b[A").unwrap();
        write_frame(&mut writer, RESIZED, &[0, 100, 0, 30]).unwrap();
        wait_until(|| shell.typed.lock().unwrap().as_slice() == b"\x1b[A");
        wait_until(|| shell.sizes.lock().unwrap().contains(&(100, 30)));
        assert_eq!(shell.sizes.lock().unwrap()[0], (80, 24));

        // Letting go puts the size it had back.
        attached.shutdown(std::net::Shutdown::Both).unwrap();
        wait_until(|| shell.sizes.lock().unwrap().last() == Some(&(120, 40)));
    }

    #[test]
    fn a_shell_that_ends_says_so_and_nothing_follows() {
        let (holder, attached) = UnixStream::pair().unwrap();
        let (_prints, output) = mpsc::channel::<Vec<u8>>();
        let shell = Arc::new(Recorder::default());
        serve_the_attachment(
            holder,
            HeldShell {
                output,
                tty: shell.clone(),
            },
            80,
            24,
        )
        .unwrap();
        shell.ended.store(true, Ordering::Relaxed);
        let mut reader = BufReader::new(attached);
        reader.read_line(&mut String::new()).unwrap();
        assert_eq!(read_frame(&mut reader).unwrap().0, ENDED);
    }
}
