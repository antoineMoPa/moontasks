//! The tools: what each is called, what it takes, and what it does.
//!
//! One for each thing the command line does to a board - see `moon tasks`, `moon wire` and
//! `moon agent` - and for what a shell does to a card by reading and writing its files. The
//! cards' own tools are in [`super::card_tools`]; the wire's and the agents' are the command
//! line's functions, called here.

use std::fs;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    board::Board,
    card_tools::{self, CreatedTask, ListedTasks, ReadTask, UpdatedTask},
};
use crate::{
    cli::{agent, wire},
    moontasks::wire::broadcasts,
    terminal::Shown,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    TasksList,
    TaskRead,
    TaskCreate,
    TaskUpdate,
    WireRead,
    WirePost,
    AgentsList,
    AgentStart,
    AgentView,
    AgentTell,
}

pub const TOOLS: &[Tool] = &[
    Tool::TasksList,
    Tool::TaskRead,
    Tool::TaskCreate,
    Tool::TaskUpdate,
    Tool::WireRead,
    Tool::WirePost,
    Tool::AgentsList,
    Tool::AgentStart,
    Tool::AgentView,
    Tool::AgentTell,
];

/// How many of the wire's lines are read when nobody says.
const WIRE_LINES_READ: usize = 20;

const TASK_ARGUMENT: &str = "The task's handle, as tasks_list prints it (`fix-the-races`, with \
or without its @). Left out, it is the task whose shell this server was started in.";

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Self::TasksList => "tasks_list",
            Self::TaskRead => "task_read",
            Self::TaskCreate => "task_create",
            Self::TaskUpdate => "task_update",
            Self::WireRead => "wire_read",
            Self::WirePost => "wire_post",
            Self::AgentsList => "agents_list",
            Self::AgentStart => "agent_start",
            Self::AgentView => "agent_view",
            Self::AgentTell => "agent_tell",
        }
    }

    /// What the tool does, in the line an agent picks it by.
    pub fn summary(self) -> &'static str {
        match self {
            Self::TasksList => {
                "List the board: every column with its cards, each by handle, title and tags. \
                 The finished column is only counted unless it is asked for or the list is \
                 filtered."
            }
            Self::TaskRead => {
                "Read one task: title, column, tags, tracker link, folder, running agents, \
                 files, review request and notes."
            }
            Self::TaskCreate => {
                "Make a card, at the top of the first column unless a column is named. \
                 Returns its handle and folder."
            }
            Self::TaskUpdate => {
                "Change a task: rename it, move it to a column, set, add or remove tags, \
                 write or append notes, link its tracker issue. Any of them in one call."
            }
            Self::WireRead => "Read the last lines the board's agents posted to each other.",
            Self::WirePost => {
                "Post one line to every agent of the board, or type it into the shells of the \
                 tasks tagged at its start (`@fix-the-races are you in src/cli?`). Only from \
                 a server started in a task's shell, which is who the line is from."
            }
            Self::AgentsList => "List the agents running on the board, by their task's handle.",
            Self::AgentStart => "Start an agent on a task, in the moon window open on this repo.",
            Self::AgentView => {
                "Read what a task's running agent is showing: its screen, or its last lines."
            }
            Self::AgentTell => "Type one line into a task's running agent and send it.",
        }
    }

    /// The JSON Schema of what the tool takes.
    fn describe_input(self) -> Value {
        let task = json!({ "type": "string", "description": TASK_ARGUMENT });
        let tags = |says: &str| json!({ "type": "array", "items": { "type": "string" }, "description": says });
        match self {
            Self::TasksList => describe_object(
                json!({
                    "column": { "type": "string", "description": "Only this column, by the name the board shows it under: `IN PROGRESS`." },
                    "tag": { "type": "string", "description": "Only cards marked with this tag." },
                    "matching": { "type": "string", "description": "Only cards whose title has this text in it, whatever its case." },
                    "limit": { "type": "integer", "minimum": 1, "description": "How many cards of each column to list, from its top. 50 when left out." },
                }),
                &[],
            ),
            Self::TaskRead => describe_object(json!({ "task": task }), &[]),
            Self::TaskCreate => describe_object(
                json!({
                    "title": { "type": "string" },
                    "column": { "type": "string", "description": "The column it is made in, by the name the board shows. The first column when left out." },
                    "tags": tags("What the card is marked with."),
                    "notes": { "type": "string", "description": "Its notes, as markdown." },
                    "appears_on": { "type": "string", "description": "A day, `2026-11-02`: the board leaves the card out until then." },
                }),
                &["title"],
            ),
            Self::TaskUpdate => describe_object(
                json!({
                    "task": task,
                    "title": { "type": "string", "description": "A new title. The handle and the folder stay as they are." },
                    "column": { "type": "string", "description": "The column to move it to, by the name tasks_list shows it under." },
                    "tags": tags("Every tag of the card, replacing the ones it has. An empty list takes them all off."),
                    "add_tags": tags("Tags to add to the ones it has."),
                    "remove_tags": tags("Tags to take off."),
                    "notes": { "type": "string", "description": "The whole of its notes, replacing what is written." },
                    "append_notes": { "type": "string", "description": "Text to write under the notes it has." },
                    "tracker_url": { "type": "string", "description": "The web address of its issue in Linear, Jira, GitHub or GitLab. Empty takes the link off." },
                }),
                &[],
            ),
            Self::WireRead => describe_object(
                json!({ "lines": { "type": "integer", "minimum": 1, "description": "How many lines from the end. 20 when left out." } }),
                &[],
            ),
            Self::WirePost => describe_object(
                json!({ "line": { "type": "string", "description": "One line. Start it with @handle to reach one task's agents instead of the whole board." } }),
                &["line"],
            ),
            Self::AgentsList => describe_object(json!({}), &[]),
            Self::AgentStart => describe_object(
                json!({ "task": task, "agent": { "type": "string", "enum": agent::agent_names() } }),
                &["agent"],
            ),
            Self::AgentView => describe_object(
                json!({
                    "task": task,
                    "lines": { "type": "integer", "minimum": 1, "description": "This many lines from the end of everything it printed, rather than its screen." },
                }),
                &[],
            ),
            Self::AgentTell => describe_object(
                json!({ "task": task, "line": { "type": "string", "description": "One line, typed and sent as it is." } }),
                &["line"],
            ),
        }
    }
}

fn describe_object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// Every tool, as `tools/list` answers.
pub fn describe_tools() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name(),
                "description": tool.summary(),
                "inputSchema": tool.describe_input(),
            })
        })
        .collect()
}

pub fn find_tool_named(name: &str) -> Option<Tool> {
    TOOLS.iter().copied().find(|tool| tool.name() == name)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArguments {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadWire {
    #[serde(default)]
    lines: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostedLine {
    line: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartedAgent {
    #[serde(default)]
    task: Option<String>,
    agent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewedAgent {
    #[serde(default)]
    task: Option<String>,
    #[serde(default)]
    lines: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToldAgent {
    #[serde(default)]
    task: Option<String>,
    line: String,
}

/// What a tool was called with, read as what it takes - or what is wrong with it, said so
/// the agent can call again.
fn read_arguments<Taken: serde::de::DeserializeOwned>(
    tool: Tool,
    arguments: Value,
) -> Result<Taken> {
    serde_json::from_value(arguments)
        .with_context(|| format!("{} was not called with what it takes", tool.name()))
}

/// Run a tool, and what it says back.
pub fn call_tool(board: &Board, tool: Tool, arguments: Value) -> Result<String> {
    match tool {
        Tool::TasksList => {
            card_tools::list_tasks(board, &read_arguments::<ListedTasks>(tool, arguments)?)
        }
        Tool::TaskRead => {
            card_tools::read_task(board, &read_arguments::<ReadTask>(tool, arguments)?)
        }
        Tool::TaskCreate => {
            card_tools::create_task(board, &read_arguments::<CreatedTask>(tool, arguments)?)
        }
        Tool::TaskUpdate => {
            card_tools::update_task(board, &read_arguments::<UpdatedTask>(tool, arguments)?)
        }
        Tool::WireRead => {
            let read: ReadWire = read_arguments(tool, arguments)?;
            read_last_wire_lines(board, read.lines.unwrap_or(WIRE_LINES_READ))
        }
        Tool::WirePost => {
            let posted: PostedLine = read_arguments(tool, arguments)?;
            wire::post_line(&posted.line, board.own_task_dir.clone())
        }
        Tool::AgentsList => {
            read_arguments::<NoArguments>(tool, arguments)?;
            agent::list(&board.repo_path)
        }
        Tool::AgentStart => {
            let started: StartedAgent = read_arguments(tool, arguments)?;
            let task = board.resolve_task_named(started.task.as_deref())?;
            agent::start(&board.folder, &task.id, agent::agent_named(&started.agent)?)
        }
        Tool::AgentView => {
            let viewed: ViewedAgent = read_arguments(tool, arguments)?;
            let task = board.resolve_task_named(viewed.task.as_deref())?;
            let wanted = viewed.lines.map_or(Shown::Screen, Shown::LastRows);
            agent::view(&board.repo_path, &task.id, wanted)
        }
        Tool::AgentTell => {
            let told: ToldAgent = read_arguments(tool, arguments)?;
            let task = board.resolve_task_named(told.task.as_deref())?;
            agent::tell(&board.repo_path, &task.id, &told.line)
        }
    }
}

/// The end of the file the board's broadcasts are kept in. A board nobody has posted on has
/// no file yet, which is said rather than refused.
fn read_last_wire_lines(board: &Board, wanted: usize) -> Result<String> {
    let path = broadcasts::file_path(&board.repo_path);
    let Ok(posted) = fs::read_to_string(&path) else {
        return Ok(format!("nothing has been posted to {}\n", path.display()));
    };
    let lines: Vec<&str> = posted
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let from = lines.len().saturating_sub(wanted.max(1));
    Ok(lines[from..]
        .iter()
        .map(|line| format!("{line}\n"))
        .collect())
}
