use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{McpCommand, board::Board, protocol::answer_message, serve_messages, tools};
use crate::moontasks::{
    ColumnEnd, ColumnId,
    store::{self, BoardColumn, BoardConfig},
};

/// A repo whose board has a queue, a column of work under way and the column that finishes
/// a task.
fn make_board_repo(name: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!(
        "moontasks-mcp-{}-{name}-{}",
        std::process::id(),
        store::new_uuid()
    ));
    std::fs::create_dir_all(&repo).expect("failed to create the test repo");
    let column = |id: &str, label: &str| BoardColumn {
        id: ColumnId::new(id),
        label: label.to_string(),
        arrivals: None,
        sort: None,
        marks_a_days_work: None,
    };
    let columns = vec![
        column("big", "TODO"),
        column("in_progress", "IN PROGRESS"),
        column("done", "DONE"),
    ];
    store::write_board(&repo, &BoardConfig { columns }).expect("expected the board");
    repo
}

fn make_card(repo: &Path, title: &str, column: &str) -> String {
    store::create_task(repo, title, &ColumnId::new(column), ColumnEnd::Bottom)
        .expect("expected the card to be made")
}

/// A server on the board of `repo`, started from the shell of `own_task` if there is one.
fn serve_board(repo: &Path, own_task: Option<&str>) -> Board {
    let task_dir = own_task.map(|task_id| {
        store::task_dir(repo, task_id)
            .expect("expected the task's folder")
            .into_os_string()
    });
    Board::with_own_task(repo.to_path_buf(), repo.to_path_buf(), task_dir)
}

/// What a tool says, and whether it refused.
fn call(board: &Board, tool: &str, arguments: Value) -> (String, bool) {
    let asked = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments },
    });
    let answer: Value = serde_json::from_str(
        &answer_message(board, &asked.to_string()).expect("expected a request to be answered"),
    )
    .expect("expected the answer to be JSON");
    assert_eq!(answer["id"], 7);
    let result = &answer["result"];
    (
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("expected text in {answer}"))
            .to_string(),
        result["isError"].as_bool().expect("expected isError"),
    )
}

fn said(board: &Board, tool: &str, arguments: Value) -> String {
    let (text, refused) = call(board, tool, arguments);
    assert!(!refused, "{tool} refused: {text}");
    text
}

fn refusal(board: &Board, tool: &str, arguments: Value) -> String {
    let (text, refused) = call(board, tool, arguments);
    assert!(refused, "{tool} did not refuse: {text}");
    text
}

#[test]
fn a_client_is_greeted_told_the_tools_and_not_answered_for_a_notification() {
    let repo = make_board_repo("hello");
    let board = serve_board(&repo, None);
    let messages = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": "three", "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "prompts/list"}),
    ]
    .map(|message| message.to_string())
    .join("\n\n");

    let mut written = Vec::new();
    serve_messages(
        &board,
        format!("{messages}\nnot json\n").as_bytes(),
        &mut written,
    )
    .expect("expected the client to be served");
    let answers: Vec<Value> = String::from_utf8(written)
        .expect("expected text")
        .lines()
        .map(|line| serde_json::from_str(line).expect("expected a JSON answer a line"))
        .collect();

    assert_eq!(answers.len(), 5, "the notification gets no answer");
    assert_eq!(answers[0]["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "moon");
    assert!(answers[0]["result"]["capabilities"]["tools"].is_object());

    let listed = answers[1]["result"]["tools"]
        .as_array()
        .expect("expected the tools");
    let names: Vec<&str> = listed
        .iter()
        .map(|tool| tool["name"].as_str().expect("expected a name"))
        .collect();
    assert_eq!(
        names,
        tools::TOOLS
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>()
    );
    for tool in listed {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(!tool["description"].as_str().unwrap_or_default().is_empty());
    }

    assert_eq!(
        answers[2],
        json!({"jsonrpc": "2.0", "id": "three", "result": {}})
    );
    assert_eq!(answers[3]["error"]["code"], -32601);
    assert_eq!(answers[4]["error"]["code"], -32700);
    assert_eq!(answers[4]["id"], Value::Null);
}

/// A version this server does not speak is offered the newest it does.
#[test]
fn an_unknown_protocol_version_is_answered_with_the_newest() {
    let board = serve_board(&make_board_repo("version"), None);
    let asked = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}});
    let answer: Value =
        serde_json::from_str(&answer_message(&board, &asked.to_string()).expect("an answer"))
            .expect("expected JSON");
    assert_eq!(answer["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn the_list_is_every_column_with_its_cards_by_handle_and_the_finished_ones_counted() {
    let repo = make_board_repo("list");
    let board = serve_board(&repo, None);
    let races = make_card(&repo, "Fix the races", "big");
    make_card(&repo, "Bing bong", "big");
    make_card(&repo, "Old work", "done");
    make_card(&repo, "Older work", "done");
    said(
        &board,
        "task_update",
        json!({"task": "fix-the-races", "tags": ["Bug", "needs tests"]}),
    );
    let _ = races;

    assert_eq!(
        said(&board, "tasks_list", json!({})),
        "TODO\n  @fix-the-races  Fix the races  #bug  #needs-tests\n  @bing-bong  Bing bong\n\
         IN PROGRESS\nDONE  (2 cards, not listed: ask for column \"DONE\")\n"
    );
    assert_eq!(
        said(&board, "tasks_list", json!({"column": "TODO", "limit": 1})),
        "TODO\n  @fix-the-races  Fix the races  #bug  #needs-tests\n  \
         ... and 1 card more: narrow the list with `tag` or `matching`, or raise `limit`\n"
    );
    assert_eq!(
        said(&board, "tasks_list", json!({"column": "DONE"})),
        "DONE\n  @old-work  Old work\n  @older-work  Older work\n"
    );
    assert_eq!(
        said(&board, "tasks_list", json!({"tag": "Needs Tests"})),
        "TODO\n  @fix-the-races  Fix the races  #bug  #needs-tests\nIN PROGRESS\nDONE\n"
    );
    assert_eq!(
        said(&board, "tasks_list", json!({"matching": "OLDER"})),
        "TODO\nIN PROGRESS\nDONE\n  @older-work  Older work\n"
    );
    assert_eq!(
        refusal(&board, "tasks_list", json!({"column": "done"})),
        "\"done\" is no column of this board: its columns are TODO, IN PROGRESS, DONE"
    );
}

#[test]
fn a_card_is_made_with_its_tags_and_notes_and_read_back_whole() {
    let repo = make_board_repo("create");
    let board = serve_board(&repo, None);

    let made = said(
        &board,
        "task_create",
        json!({
            "title": "Renew the certificate",
            "column": "IN PROGRESS",
            "tags": ["ops"],
            "notes": "It runs out in March.",
            "appears_on": "2099-01-02",
        }),
    );
    let folder = store::tasks_root(&repo);
    assert!(
        made.starts_with(&format!(
            "made @renew-the-certificate in IN PROGRESS\nfolder: {}",
            folder.display()
        )),
        "{made}"
    );

    let read = said(
        &board,
        "task_read",
        json!({"task": "@renew-the-certificate"}),
    );
    assert!(
        read.starts_with(
            "@renew-the-certificate  Renew the certificate\ncolumn: IN PROGRESS\ntags: ops\n\
             appears on: 2099-01-02\nfolder: "
        ),
        "{read}"
    );
    assert!(
        read.ends_with("\nnotes:\nIt runs out in March.\n"),
        "{read}"
    );

    assert_eq!(
        said(&board, "tasks_list", json!({"column": "IN PROGRESS"})),
        "IN PROGRESS\n  @renew-the-certificate  Renew the certificate  #ops  (appears on 2099-01-02)\n"
    );
    assert_eq!(
        refusal(&board, "task_create", json!({"title": "  "})),
        "a task needs a title"
    );
}

#[test]
fn a_task_is_renamed_tagged_noted_linked_and_moved_in_one_call() {
    let repo = make_board_repo("update");
    let board = serve_board(&repo, None);
    let races = make_card(&repo, "Fix the races", "big");
    said(
        &board,
        "task_update",
        json!({"task": "fix-the-races", "tags": ["bug", "slow"], "notes": "first"}),
    );

    assert_eq!(
        said(
            &board,
            "task_update",
            json!({
                "task": "fix-the-races",
                "title": "Fix every race",
                "add_tags": ["Needs Tests"],
                "remove_tags": ["slow"],
                "append_notes": "second",
                "tracker_url": "https://linear.app/acme/issue/BM-1/races",
                "column": "IN PROGRESS",
            })
        ),
        "renamed @fix-the-races to \"Fix every race\"\n\
         @fix-the-races is tagged bug, needs-tests\n\
         added to the notes of @fix-the-races\n\
         linked @fix-the-races to https://linear.app/acme/issue/BM-1/races\n\
         moved @fix-the-races to IN PROGRESS\n"
    );
    let card = store::read_task(&repo, &races).expect("expected the task");
    assert_eq!(card.title, "Fix every race");
    assert_eq!(card.tags, ["bug", "needs-tests"]);
    assert_eq!(card.status, Some(ColumnId::new("in_progress")));
    assert_eq!(store::read_notes(&repo, &races), "first\nsecond\n");

    assert_eq!(
        said(
            &board,
            "task_update",
            json!({"task": "fix-the-races", "tags": []})
        ),
        "@fix-the-races has no tags\n"
    );
    assert_eq!(
        refusal(&board, "task_update", json!({"task": "fix-the-races"})),
        "nothing to change on @fix-the-races: name a title, a column, tags, notes or a tracker_url"
    );
    assert!(
        refusal(
            &board,
            "task_update",
            json!({"task": "fix-the-races", "colour": "red"})
        )
        .starts_with("task_update was not called with what it takes: unknown field `colour`")
    );
    assert_eq!(
        refusal(
            &board,
            "task_update",
            json!({"task": "fix-the-races", "tracker_url": "BM-1"})
        ),
        "a tracker link starts with https:// or http://, and `BM-1` does not"
    );
}

/// From a task's own shell the task need not be named, and from anywhere else it has to be.
#[test]
fn a_tool_that_names_no_task_is_about_the_shells_own() {
    let repo = make_board_repo("own");
    let races = make_card(&repo, "Fix the races", "big");
    make_card(&repo, "Bing bong", "big");

    let in_its_shell = serve_board(&repo, Some(&races));
    assert_eq!(
        said(
            &in_its_shell,
            "task_update",
            json!({"append_notes": "half way"})
        ),
        "added to the notes of @fix-the-races\n"
    );
    assert!(said(&in_its_shell, "task_read", json!({})).ends_with("\nnotes:\nhalf way\n"));

    let outside = serve_board(&repo, None);
    assert!(refusal(&outside, "task_read", json!({})).starts_with("which task?"));
    assert!(
        refusal(&outside, "task_read", json!({"task": "no-such"})).contains("tasks_list prints")
    );
}

#[test]
fn the_wire_is_posted_to_from_a_tasks_shell_and_read_from_anywhere() {
    let repo = make_board_repo("wire");
    let races = make_card(&repo, "Fix the races", "big");
    let outside = serve_board(&repo, None);
    assert!(said(&outside, "wire_read", json!({})).starts_with("nothing has been posted to "));
    assert!(
        refusal(&outside, "wire_post", json!({"line": "rewriting src/cli"}))
            .contains("is run from a task's shell")
    );

    let in_its_shell = serve_board(&repo, Some(&races));
    for line in ["rewriting src/cli", "done with src/cli"] {
        assert!(
            said(&in_its_shell, "wire_post", json!({"line": line}))
                .starts_with("posted as @fix-the-races to ")
        );
    }
    let last = said(&outside, "wire_read", json!({"lines": 1}));
    assert!(
        last.ends_with(" @fix-the-races: done with src/cli\n"),
        "{last}"
    );
    assert_eq!(last.lines().count(), 1);
    assert_eq!(said(&outside, "wire_read", json!({})).lines().count(), 2);
}

#[test]
fn the_agents_of_a_board_with_none_running_are_said_to_be_none() {
    let repo = make_board_repo("agents");
    make_card(&repo, "Fix the races", "big");
    let board = serve_board(&repo, None);

    assert!(said(&board, "agents_list", json!({})).starts_with("no agent is running on the board"));
    assert!(
        refusal(&board, "agent_view", json!({"task": "fix-the-races"}))
            .starts_with("@fix-the-races has no running agent")
    );
    assert!(
        refusal(
            &board,
            "agent_start",
            json!({"task": "fix-the-races", "agent": "emacs"})
        )
        .starts_with("emacs is no agent moon starts")
    );
}

/// A call naming no tool is the protocol's failure, not a tool's refusal.
#[test]
fn a_call_to_no_tool_of_the_server_is_an_error() {
    let board = serve_board(&make_board_repo("no-tool"), None);
    let asked = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "task_delete"}});
    let answer: Value =
        serde_json::from_str(&answer_message(&board, &asked.to_string()).expect("an answer"))
            .expect("expected JSON");
    assert_eq!(answer["error"]["code"], -32602);
    assert_eq!(
        answer["error"]["message"],
        "task_delete is not a tool of this server"
    );
}

/// `moon mcp` is the server, on this repo's board or on the one named, and says what its
/// tools are when asked.
#[test]
fn the_mcp_server_is_started_on_this_repo_or_the_one_named() {
    let parse = |args: &[&str]| {
        let words: Vec<String> = args[1..].iter().map(|arg| arg.to_string()).collect();
        let asks_for_help = words.iter().any(|word| word == "--help");
        super::parse(&words, asks_for_help)
    };

    assert_eq!(
        parse(&["mcp"]).expect("expected it to parse"),
        McpCommand::Serve { repo: None }
    );
    assert_eq!(
        parse(&["mcp", "--repo", "/home/you/project"]).expect("expected it to parse"),
        McpCommand::Serve {
            repo: Some("/home/you/project".to_string())
        }
    );
    assert_eq!(
        parse(&["mcp", "--help"]).expect("expected it to parse"),
        McpCommand::Help
    );
    let refused = parse(&["mcp", "--remote", "dev-box"])
        .expect_err("expected a refusal")
        .to_string();
    assert!(
        refused.starts_with("`moon mcp` takes `--repo <path>` and nothing else"),
        "{refused}"
    );

    let help = super::help_text();
    for said in [
        "moon mcp --repo <path>",
        "claude mcp add moon -- moon mcp",
        r#"{ "mcpServers": { "moon": { "command": "moon", "args": ["mcp", "--repo", "/path/to/repo"] } } }"#,
    ] {
        assert!(
            help.contains(said),
            "the MCP help should say {said:?}:\n{help}"
        );
    }
    assert!(
        !help.contains("tasks_list"),
        "the tools are the server's to list"
    );
}

fn ask(board: &Board, method: &str, params: Value) -> Value {
    let asked = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    serde_json::from_str(&answer_message(board, &asked.to_string()).expect("an answer"))
        .expect("expected JSON")
}

/// The skill is said to be there, listed with the size and digest of its file, and read as
/// the file the listing described - which is what a client checks before it trusts it.
#[test]
fn the_servers_skill_is_listed_described_and_read_as_one_file() {
    use sha2::{Digest, Sha256};

    let board = serve_board(&make_board_repo("skills"), None);
    let uri = "skill://moontasks-interaction/SKILL.md";

    let hello = ask(
        &board,
        "initialize",
        json!({"protocolVersion": "2025-06-18"}),
    );
    let capabilities = &hello["result"]["capabilities"];
    assert!(capabilities["resources"].is_object());
    assert_eq!(
        capabilities["extensions"]["io.modelcontextprotocol/skills"],
        json!({})
    );
    assert!(
        hello["result"]["instructions"]
            .as_str()
            .is_some_and(|said| said.contains(uri))
    );

    let listed = ask(&board, "skills/list", json!({}));
    assert_eq!(listed["result"]["resultType"], "complete");
    assert!(listed["result"]["ttlMs"].is_u64());
    let skills = listed["result"]["skills"]
        .as_array()
        .expect("expected skills");
    assert_eq!(skills.len(), 1);
    let skill = &skills[0];
    assert_eq!(skill["uri"], uri);
    assert_eq!(skill["frontmatter"]["name"], "moontasks-interaction");
    assert!(
        skill["frontmatter"]["description"]
            .as_str()
            .is_some_and(|said| said.starts_with("Moontasks interaction - "))
    );
    assert_eq!(
        skill["frontmatter"].as_object().map(|fields| fields.len()),
        Some(2),
        "the frontmatter is its file's, and no more"
    );

    let got = ask(&board, "skills/get", json!({"uri": uri}));
    assert_eq!(&got["result"]["skill"], skill);

    let read = ask(&board, "resources/read", json!({"uri": uri}));
    let contents = &read["result"]["contents"][0];
    assert_eq!(contents["uri"], uri);
    assert_eq!(contents["mimeType"], "text/markdown");
    let text = contents["text"]
        .as_str()
        .expect("expected the skill's text");
    assert!(text.starts_with("---\nname: moontasks-interaction\ndescription: "));

    let file = &skill["resources"][0];
    assert_eq!(file["uri"], uri);
    assert_eq!(file["size"], text.len());
    let digest: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(file["digest"], format!("sha256:{digest}"));

    let resources = ask(&board, "resources/list", json!({}));
    assert_eq!(resources["result"]["resources"][0]["uri"], uri);
    assert_eq!(
        resources["result"]["resources"][0]["name"],
        "moontasks-interaction"
    );

    for (method, nowhere) in [
        ("skills/get", json!({"uri": "skill://nope/SKILL.md"})),
        (
            "resources/read",
            json!({"uri": "skill://moontasks-interaction/notes.md"}),
        ),
        ("skills/get", json!({})),
    ] {
        assert_eq!(
            ask(&board, method, nowhere)["error"]["code"],
            -32602,
            "{method}"
        );
    }
}

/// The skill is about the tools this moon has: every one of them by its name, and every
/// agent it starts.
#[test]
fn the_skill_names_every_tool_and_agent_of_the_server() {
    let board = serve_board(&make_board_repo("skill-tools"), None);
    let read = ask(
        &board,
        "resources/read",
        json!({"uri": "skill://moontasks-interaction/SKILL.md"}),
    );
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("expected the skill's text");
    for tool in tools::TOOLS {
        assert!(
            text.contains(&format!("`{}`", tool.name())),
            "the skill should name {}",
            tool.name()
        );
    }
    for agent in crate::cli::agent::agent_names() {
        assert!(
            text.contains(&format!("`{agent}`")),
            "the skill should name {agent}"
        );
    }
}
