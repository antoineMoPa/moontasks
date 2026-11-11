//! `moon wire`: a line from one agent of a board to the others - see
//! [`crate::moontasks::wire`] for what the wire is, and [`help_text`] for what an agent is
//! told about it.
//!
//! It is run from a task's shell, and reads who is posting out of the environment that
//! shell was started with - see [`super::task_shell`].

use std::{ffi::OsString, path::Path};

use anyhow::{Context, Result, bail};

use super::{PROGRAM, task_shell::TaskOfShell};
use crate::{
    instances::{self, Ask},
    moontasks::{
        TASK_DIR_ENV_VAR, store,
        wire::{self, broadcasts, direct, handles},
    },
};

/// What `moon wire …` asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum WireCommand {
    Help,
    /// `moon wire post …`: a line for the tasks tagged in front of it, or for every agent of
    /// the board when none is.
    Post {
        /// The tags the line started with, without their `@`.
        tags: Vec<String>,
        message: String,
    },
}

/// What marks a word in front of a line as a tag: `@fix-the-races`.
const TAG_MARK: char = '@';

/// Which of the wire's commands a command line names, from the words after `wire`.
///
/// `moon wire` alone is the help, as `moon` alone is: there is nothing else it could mean.
pub fn parse(args: &[String], asks_for_help: bool) -> Result<WireCommand> {
    let Some((command, words)) = args.split_first() else {
        return Ok(WireCommand::Help);
    };
    if asks_for_help {
        return Ok(WireCommand::Help);
    }
    match command.as_str() {
        "post" => parse_post(words),
        other => bail!(
            "`{PROGRAM} wire {other}` is not a command\n\n{}",
            help_text()
        ),
    }
}

/// `moon wire post <words…>`. The line is the whole of the rest, joined, so it needs no
/// quoting - though it usually gets some, and a quoted line arrives as one word with its
/// tags inside it. So the tags are read off the front of the joined line: every word that
/// starts with `@`, up to the first that does not.
fn parse_post(words: &[String]) -> Result<WireCommand> {
    let line = words.join(" ");
    let mut message = line.trim();
    let mut tags = Vec::new();
    while let Some(tagged) = message.strip_prefix(TAG_MARK) {
        let (tag, rest) = tagged
            .split_once(char::is_whitespace)
            .unwrap_or((tagged, ""));
        if tag.is_empty() {
            bail!("`{TAG_MARK}` alone tags nobody: a tag is `{TAG_MARK}handle`, with no space");
        }
        tags.push(tag.to_string());
        message = rest.trim_start();
    }
    if message.is_empty() {
        match tags.as_slice() {
            [] => bail!(
                "`{PROGRAM} wire post` needs the line to post, e.g. `{PROGRAM} wire post \
                 \"rewriting src/cli\"`"
            ),
            _ => bail!(
                "`{PROGRAM} wire post` needs a message after the tags, e.g. `{PROGRAM} wire \
                 post \"{TAG_MARK}{} are you in src/cli?\"`",
                tags[0]
            ),
        }
    }
    Ok(WireCommand::Post {
        tags,
        message: message.to_string(),
    })
}

pub fn run(command: WireCommand) -> Result<()> {
    match command {
        WireCommand::Help => {
            println!("{}", help_text());
            Ok(())
        }
        WireCommand::Post { tags, message } => {
            print!(
                "{}",
                post(&tags, &message, crate::moon_env_var_os(TASK_DIR_ENV_VAR))?
            );
            Ok(())
        }
    }
}

/// Post a line as it would be typed after `moon wire post`, its tags at its start, from the
/// task whose folder this is - for a caller with no command line to have parsed it off.
pub fn post_line(line: &str, task_dir: Option<OsString>) -> Result<String> {
    match parse_post(&[line.to_string()])? {
        WireCommand::Post { tags, message } => post(&tags, &message, task_dir),
        WireCommand::Help => unreachable!("a post is parsed as a post"),
    }
}

/// One shell a direct message is to be typed into, with everything checked that can be
/// before anything is sent.
struct Delivery {
    /// The tag as it was typed, which is how the poster knows the task.
    tag: String,
    /// The process of the moon holding the shell, a window or a `moon serve`.
    held_by: u32,
    line: Ask,
}

/// Post a line: to the board's file when it has no tags, and into the shells of the tagged
/// tasks' agents when it has. What comes back is what the poster is told, a line for each place
/// the line went.
pub fn post(tags: &[String], message: &str, task_dir: Option<OsString>) -> Result<String> {
    let sender = TaskOfShell::of(&wire::post_command(), task_dir.as_deref())?;
    let message = wire::one_line(message)?;
    let folders = sender.folders_of_its_board()?;
    let handle = handles::handle_of(&sender.task_id, &folders);

    if tags.is_empty() {
        // The file is written here, by this command, so whose agent this is comes from what
        // the shell was told. A direct message is signed by the server that types it.
        let person = instances::shell_person();
        let signed = wire::from_the_agent_of(person.as_deref(), message);
        broadcasts::post(&sender.repo_path, &handle, &signed)?;
        return Ok(format!(
            "posted as {TAG_MARK}{handle} to {}\n",
            broadcasts::file_path(&sender.repo_path).display()
        ));
    }

    let deliveries = deliveries(&sender.repo_path, &folders, &handle, tags, message)?;
    let mut said = String::new();
    for delivery in deliveries {
        instances::wire(delivery.held_by, &delivery.line)
            .with_context(|| format!("{TAG_MARK}{} was not sent the line", delivery.tag))?;
        said.push_str(&format!("sent to {TAG_MARK}{}\n", delivery.tag));
    }
    Ok(said)
}

/// Every shell a direct message goes to, or the first reason it cannot go to one of them.
///
/// Worked out whole before anything is sent, so a tag that names nobody, or a task nobody is
/// running in, leaves every other tagged agent untold rather than half of them told.
fn deliveries(
    repo_path: &Path,
    folders: &[String],
    sender: &str,
    tags: &[String],
    message: &str,
) -> Result<Vec<Delivery>> {
    let mut tagged: Vec<(&String, &str)> = Vec::new();
    for tag in tags {
        let task_id = match handles::resolve(tag, folders) {
            Ok(task_id) => task_id,
            Err(unknown @ handles::Unresolved::Unknown { .. }) => {
                bail!("{unknown}. {}", agents_to_tag(repo_path, folders, sender)?)
            }
            Err(ambiguous) => return Err(ambiguous.into()),
        };
        // A task tagged twice is told once.
        if !tagged.iter().any(|(_, already)| *already == task_id) {
            tagged.push((tag, task_id));
        }
    }

    let mut deliveries = Vec::new();
    for (tag, task_id) in tagged {
        let recipient = handles::handle_of(task_id, folders);
        let agents = direct::running_agents(&store::read_task(repo_path, task_id)?);
        if agents.is_empty() {
            bail!("{TAG_MARK}{tag} has no running agent");
        }
        for agent in agents {
            deliveries.push(Delivery {
                tag: tag.clone(),
                held_by: agent.held_by,
                line: Ask::Wire {
                    terminal_id: agent.terminal_id,
                    sender: sender.to_string(),
                    recipient: recipient.clone(),
                    message: message.to_string(),
                },
            });
        }
    }
    Ok(deliveries)
}

/// Who there is to tag, said to a poster whose tag named nobody: the board's other tasks
/// with an agent running. Not every task of the board - a board keeps its finished cards,
/// and theirs are handles nobody answers to.
fn agents_to_tag(repo_path: &Path, folders: &[String], sender: &str) -> Result<String> {
    let mut running = Vec::new();
    for folder in folders {
        if direct::running_agents(&store::read_task(repo_path, folder)?).is_empty() {
            continue;
        }
        let handle = handles::handle_of(folder, folders);
        if handle != sender {
            running.push(format!("{TAG_MARK}{handle}"));
        }
    }
    Ok(match running.as_slice() {
        [] => "No other task of it has an agent running".to_string(),
        _ => format!("Its tasks with an agent running: {}", running.join(", ")),
    })
}

/// `moon wire --help`. The wire's rules are in [`crate::moontasks::wire`], and a post that
/// breaks one is refused with the rule it broke.
pub fn help_text() -> String {
    format!(
        "{PROGRAM} wire

Tells the agents working on this board what you are doing: all of them, or one by its @handle.

Usage:
  {PROGRAM} wire post <one line>
  {PROGRAM} wire post @<handle> <one line>
  {PROGRAM} wire --help

Examples:
  {PROGRAM} wire post \"rewriting src/cli/args.rs and its tests\"
  {PROGRAM} wire post \"@fix-the-races are you still in src/terminal.rs?\""
    )
}
