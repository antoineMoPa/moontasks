//! An agent's control socket: what a Minerve run tells moon about its conversation, beside
//! its pty.
//!
//! Before Minerve is started, moon listens on a Unix socket and names its path in
//! [`MINERVE_CONTROL_SOCKET_ENV_VAR`]. Minerve connects as it starts, and from then on both
//! sides write one JSON object per line. The protocol is written down in minerve's
//! `src/control_socket.rs`; this is the half of it moon reads:
//!
//! - every message of the conversation, kept here for the window's inspect pane - see
//!   [`crate::native::conversation_pane`];
//! - when a turn starts and ends. A turn that ends is the agent waiting to be typed at, which
//!   is put on the shell as an ask for attention, the way the other agents say it through the
//!   terminal - see [`crate::attention`].
//!
//! The file descriptor a pty's child could have inherited instead is closed by `portable-pty`
//! before the program runs, which is why it is a path. Only one connection is taken, and the
//! path is removed as soon as it is, so nothing else can connect after the agent.
//!
//! A line moon cannot read is a Minerve speaking another protocol: the conversation stops
//! being read, says why, and the shell carries on. Events moon has no use for - what
//! `--serve` says for a window to draw (transcript, usage, ...) - are passed over.
//!
//! A Minerve started with `--serve` has nothing on its pty: the window draws minerve's own view
//! instead, fed every line through [`AgentControl::tap`] and writing its commands with
//! [`AgentControl::send_line`] - see `crate::native::minerve_pane`.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use super::TerminalSession;
use crate::{
    agent_conversation::{AgentConversation, AgentMessage, AgentState},
    attention::Asked,
};

pub const MINERVE_CONTROL_SOCKET_ENV_VAR: &str = "MINERVE_CONTROL_SOCKET";

/// The protocol version moon reads. Minerve says which it speaks in its `hello`.
const PROTOCOL_VERSION: u32 = 1;

/// What moon puts on the shell when Minerve finishes a turn - and what
/// [`crate::attention::WAITING_TO_BE_TYPED_AT`] reads as waiting at its box.
pub const WAITING_FOR_INPUT_NOTIFICATION: &str = "Minerve is waiting for your input";

/// How long the agent has to connect. A Minerve too old to know the socket never does, and
/// the listener is let go of rather than waited on for as long as the shell runs.
const CONNECTS_WITHIN: Duration = Duration::from_secs(60);
const LOOKS_FOR_CONNECTION_EVERY: Duration = Duration::from_millis(100);

/// A line Minerve writes.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MinerveEvent {
    Hello {
        version: u32,
    },
    Session {
        session_id: String,
    },
    State {
        state: AgentState,
    },
    Message {
        message: AgentMessage,
    },
    History {
        session_id: String,
        messages: Vec<AgentMessage>,
    },
    /// What a window drawing minerve reads, and moon does not.
    #[serde(other)]
    Other,
}

/// The `type`s of the lines a view that taps in late is given first, latest of each, in this
/// order: enough to draw the chat as it stands. `hello` is what the view waits for.
const REPLAYED: [&str; 6] = ["hello", "session", "intro", "transcript", "state", "usage"];

/// The views reading a Minerve's lines, and the latest of the lines a new one is given first.
#[derive(Default)]
struct Taps {
    views: Vec<std::sync::mpsc::Sender<String>>,
    latest: Vec<(&'static str, String)>,
    /// The agent hung up, or never connected.
    ended: bool,
}

/// The `type` of an event line, without reading the rest of it.
fn type_of(line: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Typed {
        #[serde(rename = "type")]
        kind: String,
    }
    serde_json::from_str::<Typed>(line)
        .ok()
        .map(|typed| typed.kind)
}

/// A line moon writes.
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ParentCommand {
    GetHistory {},
    Submit { text: String },
}

pub struct AgentControl {
    socket_path: PathBuf,
    conversation: Mutex<AgentConversation>,
    writer: Mutex<Option<UnixStream>>,
    taps: Mutex<Taps>,
    /// Started with `--serve`, for a window to draw - see [`Self::tap`].
    serves_a_view: bool,
    /// What a task's run would have typed in a terminal agent's box, for the first view to put
    /// in minerve's - see [`Self::take_opening_line`].
    opening_line: Mutex<Option<String>>,
}

impl AgentControl {
    /// Listens on a fresh socket, for a run that is about to be started with its path in
    /// [`MINERVE_CONTROL_SOCKET_ENV_VAR`].
    pub fn listen(serves_a_view: bool) -> anyhow::Result<(Arc<Self>, UnixListener)> {
        // Short, because a socket path has 104 bytes on macOS, whose temporary folder is long.
        let dir = std::env::temp_dir().join("moon-ctl");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        // A folder left by somebody else is not one to put this user's agent's socket in.
        anyhow::ensure!(
            std::fs::metadata(&dir)?.permissions().mode() & 0o077 == 0,
            "{} can be entered by other users",
            dir.display()
        );
        let mut name = [0u8; 6];
        getrandom_03::fill(&mut name).expect("the OS has no randomness to name a socket with");
        let name: String = name.iter().map(|byte| format!("{byte:02x}")).collect();
        let socket_path = dir.join(format!("{name}.sock"));
        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        let control = Arc::new(Self {
            socket_path,
            conversation: Mutex::new(AgentConversation::default()),
            writer: Mutex::new(None),
            taps: Mutex::new(Taps::default()),
            serves_a_view,
            opening_line: Mutex::new(None),
        });
        Ok((control, listener))
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn conversation(&self) -> AgentConversation {
        self.conversation.lock().unwrap().clone()
    }

    /// Takes the agent's connection and reads it, on a thread of its own, until the agent
    /// hangs up. Weak, so a shell that is let go of is not kept alive by its reader.
    pub fn read_in_background(
        self: &Arc<Self>,
        listener: UnixListener,
        session: Weak<TerminalSession>,
    ) {
        let control = Arc::clone(self);
        std::thread::spawn(move || {
            let stream = accept_within(&listener, CONNECTS_WITHIN);
            let _ = std::fs::remove_file(&control.socket_path);
            drop(listener);
            let Some(stream) = stream else {
                control.hang_up();
                return;
            };
            if stream.set_nonblocking(false).is_err() {
                control.hang_up();
                return;
            }
            let Ok(writer) = stream.try_clone() else {
                control.hang_up();
                return;
            };
            *control.writer.lock().unwrap() = Some(writer);
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                control.relay(&line);
                if let Err(error) = control.read_line(&line, &session) {
                    control.conversation.lock().unwrap().broken = Some(error.to_string());
                    // A `--serve` Minerve ends when its socket closes: its view goes on reading
                    // what moon could not.
                    if !control.serves_a_view {
                        break;
                    }
                }
            }
            *control.writer.lock().unwrap() = None;
            control.hang_up();
        });
    }

    fn read_line(&self, line: &str, session: &Weak<TerminalSession>) -> anyhow::Result<()> {
        let event: MinerveEvent = serde_json::from_str(line)
            .map_err(|error| anyhow::anyhow!("unreadable line from Minerve ({error}): {line}"))?;
        let mut conversation = self.conversation.lock().unwrap();
        match event {
            MinerveEvent::Hello { version } => anyhow::ensure!(
                version == PROTOCOL_VERSION,
                "Minerve speaks version {version} of its control protocol, and moon reads {PROTOCOL_VERSION}"
            ),
            // A resumed session comes with messages of its own, so the conversation is asked
            // for whole. Messages after the answer are added to it.
            MinerveEvent::Session { session_id } => {
                conversation.session_id = Some(session_id);
                conversation.messages.clear();
                drop(conversation);
                self.send(&ParentCommand::GetHistory {});
            }
            MinerveEvent::History {
                session_id,
                messages,
            } => {
                conversation.session_id = Some(session_id);
                conversation.messages = messages;
            }
            MinerveEvent::Message { message } => conversation.messages.push(message),
            MinerveEvent::Other => {}
            MinerveEvent::State { state } => {
                let turn_ended = conversation.state == Some(AgentState::Working)
                    && state == AgentState::WaitingForInput;
                conversation.state = Some(state);
                drop(conversation);
                if turn_ended && let Some(session) = session.upgrade() {
                    session.asked_for_attention(Asked::Notification(
                        WAITING_FOR_INPUT_NOTIFICATION.to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn set_opening_line(&self, line: String) {
        *self.opening_line.lock().unwrap() = Some(line);
    }

    /// The opening line, for the first view only: one put back later finds the box as it was.
    pub fn take_opening_line(&self) -> Option<String> {
        self.opening_line.lock().unwrap().take()
    }

    /// The agent is gone, or never came: the views hear it, and so does one tapping in later.
    fn hang_up(&self) {
        let mut taps = self.taps.lock().unwrap();
        taps.views.clear();
        taps.ended = true;
    }

    /// Hands `line` to every view, and keeps it if a late view is to be given it first.
    fn relay(&self, line: &str) {
        if !self.serves_a_view {
            return;
        }
        let mut taps = self.taps.lock().unwrap();
        if let Some(kind) = type_of(line)
            && let Some(kept) = REPLAYED.iter().find(|kept| **kept == kind)
        {
            taps.latest.retain(|(of, _)| of != kept);
            taps.latest.push((kept, line.to_string()));
        }
        taps.views
            .retain(|view| view.send(line.to_string()).is_ok());
    }

    /// Every line this Minerve says from now on, after the latest of those a view needs to
    /// draw the chat as it stands. `None` for one not started for a view.
    pub fn tap(&self) -> Option<std::sync::mpsc::Receiver<String>> {
        if !self.serves_a_view {
            return None;
        }
        let (view, lines) = std::sync::mpsc::channel();
        let mut taps = self.taps.lock().unwrap();
        for kind in REPLAYED {
            if let Some((_, line)) = taps.latest.iter().find(|(of, _)| *of == kind) {
                let _ = view.send(line.clone());
            }
        }
        // Hung up already: the view hears it straight away.
        if !taps.ended {
            taps.views.push(view);
        }
        Some(lines)
    }

    /// Writes one of a view's command lines to the agent, as the view wrote it.
    /// Under the writer's lock, as moon's own commands are, so lines never interleave.
    pub fn send_line(&self, line: &str) {
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            let _ = writer.write_all(format!("{line}\n").as_bytes());
        }
    }

    /// Sends `text` as a prompt, the way pressing Enter in the agent's box would. Refused
    /// while a turn runs, as Minerve would refuse it.
    pub fn submit(&self, text: &str) -> anyhow::Result<()> {
        let state = self.conversation.lock().unwrap().state;
        anyhow::ensure!(
            state == Some(AgentState::WaitingForInput),
            "Minerve is not waiting for a prompt"
        );
        self.write(&ParentCommand::Submit {
            text: text.to_string(),
        })
    }

    /// Writes a command to the agent. One that has hung up is not told anything.
    fn send(&self, command: &ParentCommand) {
        let _ = self.write(command);
    }

    fn write(&self, command: &ParentCommand) -> anyhow::Result<()> {
        let mut line = serde_json::to_vec(command).expect("commands always serialize");
        line.push(b'\n');
        let mut writer = self.writer.lock().unwrap();
        let writer = writer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Minerve is not connected"))?;
        writer.write_all(&line)?;
        Ok(())
    }
}

impl Drop for AgentControl {
    /// A socket nobody connected to is still on disk.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn accept_within(listener: &UnixListener, within: Duration) -> Option<UnixStream> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(LOOKS_FOR_CONNECTION_EVERY)
            }
            Err(_) => return None,
        }
    }
    None
}

#[cfg(test)]
#[path = "agent_control_tests.rs"]
mod tests;
