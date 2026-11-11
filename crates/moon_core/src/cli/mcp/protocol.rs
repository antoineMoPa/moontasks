//! The Model Context Protocol, as much of it as a server of tools speaks: JSON-RPC 2.0, a
//! message per line.
//!
//! A client says hello with `initialize`, asks what there is with `tools/list` and calls one
//! with `tools/call`. The skills of [`super::skills`] are listed and read here too. A message with an `id` is a request and gets one answer; one without
//! is a notification and gets none.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{board::Board, skills, tools};

/// The versions of the protocol this server speaks, newest first. A client asking for one
/// of them is answered in it, and any other is offered the newest.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC's own numbers for what went wrong.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What a client is told this server is for, once, when it says hello.
const INSTRUCTIONS: &str = "The task board of this repo: its columns and cards, each card's \
notes and tags, the wire its agents tell each other things on, and the agents running on its \
tasks. A task is named by its handle, as tasks_list prints it. A server started from a task's \
own shell takes that task when none is named.";

/// One message from the client. Its answer to something of ours has no `method`, and
/// nothing is ever asked of a client, so such a message is dropped.
#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

/// What `tools/call` is asked with.
#[derive(Deserialize)]
struct ToolCall {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

/// The line to write back for a line the client wrote, or nothing for a notification.
pub fn answer_message(board: &Board, line: &str) -> Option<String> {
    let message: Message = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => {
            return Some(error_answer(
                Value::Null,
                PARSE_ERROR,
                &format!("not a JSON-RPC message: {error}"),
            ));
        }
    };
    let Some(method) = message.method else {
        return None;
    };
    // A notification: `notifications/initialized`, a cancellation. Nothing to say back.
    let id = message.id?;

    let answered = match method.as_str() {
        "initialize" => Ok(describe_server(message.params.as_ref())),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::describe_tools() })),
        "tools/call" => call_tool(board, message.params),
        "skills/list" => Ok(skills::list_skills()),
        "skills/get" => skills::describe_skill_at(message.params.as_ref()).map_err(refuse_params),
        "resources/list" => Ok(skills::list_resources()),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "resources/read" => {
            skills::read_resource_at(message.params.as_ref()).map_err(refuse_params)
        }
        other => Err((
            METHOD_NOT_FOUND,
            format!("{other} is not a method of this server"),
        )),
    };
    Some(match answered {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        Err((code, said)) => error_answer(id, code, &said),
    })
}

/// What was asked for is not there to give: JSON-RPC's invalid params.
fn refuse_params(said: String) -> (i64, String) {
    (INVALID_PARAMS, said)
}

fn error_answer(id: Value, code: i64, said: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": said } }).to_string()
}

/// The answer to `initialize`: the version spoken from here on, and that there are tools.
fn describe_server(params: Option<&Value>) -> Value {
    let asked_for = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|version| Some(**version) == asked_for)
        .unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": {},
            // The skills are read as resources, and listed by the extension's own methods.
            "resources": {},
            "extensions": { skills::EXTENSION: {} },
        },
        "serverInfo": { "name": "moon", "version": env!("CARGO_PKG_VERSION") },
        "instructions": format!(
            "{INSTRUCTIONS} How to use the tools well is a skill of this server: {}.",
            skills::list_skill_uris().join(", ")
        ),
    })
}

/// Run a tool and wrap what it said. A tool that refuses is an answer the agent reads and
/// acts on - `isError` - rather than a failure of the protocol; only a call naming no tool
/// is that.
fn call_tool(board: &Board, params: Option<Value>) -> Result<Value, (i64, String)> {
    let call: ToolCall = serde_json::from_value(params.unwrap_or(Value::Null))
        .map_err(|error| (INVALID_REQUEST, format!("not a tool call: {error}")))?;
    let tool = tools::find_tool_named(&call.name).ok_or_else(|| {
        (
            INVALID_PARAMS,
            format!("{} is not a tool of this server", call.name),
        )
    })?;
    let arguments = call.arguments.unwrap_or_else(|| json!({}));
    let (text, refused) = match tools::call_tool(board, tool, arguments) {
        Ok(said) => (said, false),
        Err(error) => (format!("{error:#}"), true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": refused }))
}
