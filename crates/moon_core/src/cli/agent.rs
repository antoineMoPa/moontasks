//! `moon agent`: the agents of a board listed, and one started, looked at and told
//! something, from a command line rather than from the board's window - see [`help_text`] for what somebody running it
//! is told.
//!
//! It is run in the repo whose board the task is on, the way `moon tasks new` is, and a task
//! is named by its handle, the way the wire names one - see [`handles`]. None of it is done
//! here: an agent runs in a shell a moon holds - a window, or a `moon serve` - so each
//! command finds that moon and asks it - see [`crate::instances`].

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::PROGRAM;
use crate::{
    api::AgentKind,
    git::project_root,
    instances,
    moontasks::{
        AGENT_LAUNCHES,
        store::{self, TaskMetadata},
        wire::{self, direct, handles},
    },
    terminal::Shown,
};

/// What `moon agent …` asked for. A task is carried as the handle that was typed, without
/// its `@`.
#[derive(Debug, PartialEq, Eq)]
pub enum AgentCommand {
    Help,
    /// `moon agent list`.
    List,
    /// `moon agent start <task> <agent>`.
    Start {
        task: String,
        agent: AgentKind,
    },
    /// `moon agent view <task> [--lines <n>]`.
    View {
        task: String,
        wanted: Shown,
    },
    /// `moon agent tell <task> <one line>`.
    Tell {
        task: String,
        line: String,
    },
}

/// What a handle may be written with in front of it, as it is on the wire: `@fix-the-races`.
const HANDLE_MARK: char = '@';

/// The option of `view` that asks for rows from the bottom of everything the agent has
/// printed, rather than for its screen.
const LINES_OPTION: &str = "--lines";

/// Which of the commands a command line names, from the words after `agent`.
///
/// `moon agent` alone is the help, as `moon wire` alone is.
pub fn parse(args: &[String], asks_for_help: bool) -> Result<AgentCommand> {
    let Some((command, words)) = args.split_first() else {
        return Ok(AgentCommand::Help);
    };
    if asks_for_help {
        return Ok(AgentCommand::Help);
    }
    match command.as_str() {
        "list" if words.is_empty() => Ok(AgentCommand::List),
        "list" => bail!(
            "`{PROGRAM} agent list` lists the agents running on this repo's board, so it takes \
             nothing"
        ),
        "start" => parse_start(words),
        "view" => parse_view(words),
        "tell" => parse_tell(words),
        other => bail!(
            "`{PROGRAM} agent {other}` is not a command\n\n{}",
            help_text()
        ),
    }
}

/// The handle a task was named by, without the `@` it may have been typed with.
fn handle(word: &str) -> Result<String> {
    let handle = word.strip_prefix(HANDLE_MARK).unwrap_or(word);
    if handle.is_empty() || handle.starts_with('-') {
        bail!("{word:?} names no task: a task is named by its handle, as in `fix-the-races`");
    }
    Ok(handle.to_string())
}

fn parse_start(words: &[String]) -> Result<AgentCommand> {
    let [task, agent] = words else {
        bail!(
            "`{PROGRAM} agent start` takes the task and the agent to start on it, e.g. \
             `{PROGRAM} agent start fix-the-races {}`",
            agent_names()[0]
        );
    };
    Ok(AgentCommand::Start {
        task: handle(task)?,
        agent: agent_named(agent)?,
    })
}

fn parse_view(words: &[String]) -> Result<AgentCommand> {
    let usage = || {
        format!(
            "`{PROGRAM} agent view` takes the task, and `{LINES_OPTION} <n>` for its last n \
             lines rather than its screen, e.g. `{PROGRAM} agent view fix-the-races \
             {LINES_OPTION} 40`"
        )
    };
    let (task, wanted) = match words {
        [task] => (task, Shown::Screen),
        [task, option, count] if option == LINES_OPTION => (task, Shown::LastRows(rows(count)?)),
        [task, option] => {
            let count = option
                .strip_prefix(LINES_OPTION)
                .and_then(|count| count.strip_prefix('='))
                .with_context(usage)?;
            (task, Shown::LastRows(rows(count)?))
        }
        _ => bail!(usage()),
    };
    Ok(AgentCommand::View {
        task: handle(task)?,
        wanted,
    })
}

/// How many lines `--lines` asked for, which is at least one.
fn rows(count: &str) -> Result<usize> {
    match count.parse::<usize>() {
        Ok(count) if count > 0 => Ok(count),
        _ => bail!("`{LINES_OPTION}` takes how many lines to show, and {count:?} is not a count"),
    }
}

/// `moon agent tell <task> <words…>`. The line is the whole of the rest, joined, so it needs
/// no quoting - though it usually gets some.
fn parse_tell(words: &[String]) -> Result<AgentCommand> {
    let needs_both = || {
        format!(
            "`{PROGRAM} agent tell` takes the task and the line to tell its agent, e.g. \
             `{PROGRAM} agent tell fix-the-races \"start with the tests\"`"
        )
    };
    let (task, line) = words.split_first().with_context(needs_both)?;
    let line = line.join(" ");
    if line.trim().is_empty() {
        bail!(needs_both());
    }
    Ok(AgentCommand::Tell {
        task: handle(task)?,
        line: line.trim().to_string(),
    })
}

/// What an agent is called on the command line: `claude`, `opencode`.
fn name_of(agent: AgentKind) -> String {
    agent.label().to_lowercase()
}

/// Every agent moon starts, by that name.
pub fn agent_names() -> Vec<String> {
    AGENT_LAUNCHES
        .iter()
        .map(|launch| name_of(launch.kind))
        .collect()
}

pub fn agent_named(name: &str) -> Result<AgentKind> {
    AGENT_LAUNCHES
        .iter()
        .map(|launch| launch.kind)
        .find(|agent| name_of(*agent) == name)
        .with_context(|| {
            format!(
                "{name} is no agent {PROGRAM} starts: it starts {}",
                agent_names().join(", ")
            )
        })
}

pub fn run(command: AgentCommand) -> Result<()> {
    if command == AgentCommand::Help {
        println!("{}", help_text());
        return Ok(());
    }
    let folder =
        project_root(&std::env::current_dir().context("failed to read the current directory")?)?;
    // The checkout the board is in, which is not this folder in a person's own work tree.
    let repo_path = store::board_checkout(&folder)?;
    let said = match command {
        AgentCommand::Help => unreachable!("the help was printed above"),
        AgentCommand::List => list(&repo_path)?,
        AgentCommand::Start { task, agent } => start(&folder, &task, agent)?,
        AgentCommand::View { task, wanted } => view(&repo_path, &task, wanted)?,
        AgentCommand::Tell { task, line } => tell(&repo_path, &task, &line)?,
    };
    // What a shell is showing ends in the line break of its last row.
    print!("{said}");
    Ok(())
}

/// A task of the board in a repo, as one of these commands named it.
struct Task {
    id: String,
    /// Its handle on this board, which is what is said back - the one that was typed may be
    /// a longer start of the folder's name.
    handle: String,
    metadata: TaskMetadata,
}

impl Task {
    fn named(repo_path: &Path, typed: &str) -> Result<Self> {
        let folders = store::list_task_ids(repo_path)?;
        let id = match handles::resolve(typed, &folders) {
            Ok(id) => id,
            Err(unknown @ handles::Unresolved::Unknown { .. }) => {
                bail!("{unknown}, {}", store::tasks_root(repo_path).display())
            }
            Err(ambiguous) => return Err(ambiguous.into()),
        };
        Self::in_folder(repo_path, id, &folders)
    }

    /// The task in one of the board's folders, which are all of `folders`.
    fn in_folder(repo_path: &Path, id: &str, folders: &[String]) -> Result<Self> {
        Ok(Self {
            id: id.to_string(),
            handle: handles::handle_of(id, folders),
            metadata: store::read_task(repo_path, id)?,
        })
    }

    /// The task's agents that are running, each with the moon holding its shell - or why
    /// there is nothing to tell or to look at.
    fn running_agents(&self) -> Result<Vec<Running>> {
        let handle = &self.handle;
        let agents = direct::running_agents(&self.metadata);
        if agents.is_empty() {
            bail!(
                "{HANDLE_MARK}{handle} has no running agent: start one with `{PROGRAM} agent \
                 start {handle} <agent>`"
            );
        }
        Ok(agents
            .into_iter()
            .map(|agent| Running {
                run: self.run_in(&agent.terminal_id),
                terminal_id: agent.terminal_id,
                held_by: agent.held_by,
            })
            .collect())
    }

    /// What the run in a shell is called, and the shell's own id for a run written down
    /// before runs were named.
    fn run_in(&self, terminal_id: &str) -> String {
        self.metadata
            .resources
            .iter()
            .find(|resource| resource.terminal_id.as_deref() == Some(terminal_id))
            .and_then(|resource| resource.name.clone())
            .unwrap_or_else(|| terminal_id.to_string())
    }
}

/// One running agent of a task, as it is reached.
struct Running {
    /// What its run is called - `fix the races claude - 1`.
    run: String,
    terminal_id: String,
    /// The process of the moon holding its shell, a window or a `moon serve`.
    held_by: u32,
}

/// Every agent running on the board in a repo, a line each: the handle of its task, which is
/// what `view` and `tell` take, and what its run is called. In the order the board reads its
/// tasks in, and a task's agents in the order they were started.
///
/// An agent held by a moon that does not answer is listed with the rest: it is running,
/// though nothing here can tell it anything.
pub fn list(repo_path: &Path) -> Result<String> {
    let folders = store::list_task_ids(repo_path)?;
    let mut running: Vec<(String, String)> = Vec::new();
    for folder in &folders {
        let task = Task::in_folder(repo_path, folder, &folders)?;
        for agent in direct::running_agents(&task.metadata) {
            running.push((
                format!("{HANDLE_MARK}{}", task.handle),
                task.run_in(&agent.terminal_id),
            ));
        }
    }
    if running.is_empty() {
        return Ok(format!(
            "no agent is running on the board in {}\n",
            store::tasks_root(repo_path).display()
        ));
    }
    let widest = running
        .iter()
        .map(|(handle, _)| handle.chars().count())
        .max()
        .expect("an agent is running");
    Ok(running
        .iter()
        .map(|(handle, run)| format!("{handle:<widest$}  {run}\n"))
        .collect())
}

/// Start an agent on a task, in the window open on the board's repo - or in the `moon serve`
/// this shell is a tab of - and say what its run is called. `folder` is the one this shell is
/// in, which the agent comes up in: the board's repo, or a person's own work tree of it.
pub fn start(folder: &Path, task: &str, agent: AgentKind) -> Result<String> {
    let task = Task::named(&store::board_checkout(folder)?, task)?;
    let moon = instances::moon_for_board(folder).with_context(|| {
        format!(
            "no {PROGRAM} window is open on {}: an agent runs in a shell a window holds, so \
             open one there with `{PROGRAM} tasks`",
            folder.display()
        )
    })?;
    let run = instances::start_agent(moon, folder, &task.id, agent)?;
    Ok(format!("started {run} on {HANDLE_MARK}{}\n", task.handle))
}

/// What a task's agent is showing. A task with several running shows each under the name of
/// its run.
pub fn view(repo_path: &Path, task: &str, wanted: Shown) -> Result<String> {
    let running = Task::named(repo_path, task)?.running_agents()?;
    let mut shown = Vec::new();
    for agent in &running {
        shown.push(instances::shown(agent.held_by, &agent.terminal_id, wanted)?);
    }
    if let [only] = shown.as_slice() {
        return Ok(only.clone());
    }
    Ok(running
        .iter()
        .zip(shown)
        .map(|(agent, shown)| format!("== {} ==\n{shown}", agent.run))
        .collect())
}

/// Type a line into the shell of each of a task's running agents, and send it.
pub fn tell(repo_path: &Path, task: &str, line: &str) -> Result<String> {
    let line = wire::one_line(line)?;
    let running = Task::named(repo_path, task)?.running_agents()?;
    let mut told = String::new();
    for agent in running {
        instances::tell(agent.held_by, &agent.terminal_id, line)
            .with_context(|| format!("{} was not told the line", agent.run))?;
        told.push_str(&format!("told {}\n", agent.run));
    }
    Ok(told)
}

/// `moon agent --help`.
pub fn help_text() -> String {
    let agents = agent_names().join("|");
    format!(
        "{PROGRAM} agent

Lists, starts, views or tells the agents of this repo's board.

Usage:
  {PROGRAM} agent list
  {PROGRAM} agent start <task> <{agents}>
  {PROGRAM} agent view <task> [{LINES_OPTION} <n>]
  {PROGRAM} agent tell <task> <one line>
  {PROGRAM} agent --help

Examples:
  {PROGRAM} agent list
  {PROGRAM} agent start fix-the-races claude
  {PROGRAM} agent tell fix-the-races \"start with the tests in src/terminal\"
  {PROGRAM} agent view fix-the-races
  {PROGRAM} agent view fix-the-races {LINES_OPTION} 80"
    )
}
