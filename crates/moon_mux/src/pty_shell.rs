//! This module starts shells on PTYs of this process, and leads a tab to them.

use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use anyhow::{Context, Result, anyhow};
use moon_tui_terminal::ShellLink;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Holds a shell running on a PTY: what types into it, what resizes it, and whether it ended.
pub struct PtyShell {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    ended: Arc<AtomicBool>,
}

impl PtyShell {
    /// Starts `program` - the user's shell when none is given - in `folder`, and returns it
    /// with what it prints.
    pub fn start(
        program: Option<&[String]>,
        folder: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<(mpsc::Receiver<Vec<u8>>, Arc<Self>)> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| anyhow!("failed to open a PTY: {error}"))?;
        let mut command = match program {
            Some([first, rest @ ..]) => {
                let mut command = CommandBuilder::new(first);
                command.args(rest);
                command
            }
            _ => CommandBuilder::new(
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            ),
        };
        command.cwd(folder);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| anyhow!("failed to start the shell: {error}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| anyhow!("failed to read the PTY: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| anyhow!("failed to write to the PTY: {error}"))?;
        let ended = Arc::new(AtomicBool::new(false));
        let (printed, output) = mpsc::channel();
        let ended_by_reader = Arc::clone(&ended);
        std::thread::Builder::new()
            .name("moon-mux-pty".to_string())
            .spawn(move || {
                let mut buffer = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            if printed.send(buffer[..read].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
                ended_by_reader.store(true, Ordering::Relaxed);
            })
            .context("failed to start the PTY's reader")?;
        Ok((
            output,
            Arc::new(Self {
                master: Mutex::new(pair.master),
                writer: Mutex::new(writer),
                child: Mutex::new(child),
                ended,
            }),
        ))
    }
}

impl ShellLink for PtyShell {
    fn write_input(&self, bytes: &[u8]) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("the PTY's writer is poisoned"))?;
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    fn write_reply(&self, bytes: &[u8]) -> Result<()> {
        self.write_input(bytes)
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .lock()
            .map_err(|_| anyhow!("the PTY is poisoned"))?
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| anyhow!("failed to resize the PTY: {error}"))
    }

    fn has_exited(&self) -> bool {
        if self.ended.load(Ordering::Relaxed) {
            return true;
        }
        self.child
            .lock()
            .ok()
            .and_then(|mut child| child.try_wait().ok().flatten())
            .is_some()
    }
}

impl Drop for PtyShell {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
