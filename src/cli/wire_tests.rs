//! `moon wire`'s tests that need a window to answer them - see [`moon_core::cli::wire`].

use std::path::PathBuf;

use moon_core::cli::wire::*;
#[allow(unused_imports)]
use std::{ffi::OsString, path::Path};
#[allow(unused_imports)]
use anyhow::{Context, Result, bail};
#[allow(unused_imports)]
use crate::{
    instances::{self, Ask},
    moontasks::{
        TASK_DIR_ENV_VAR, store,
        wire::{self, broadcasts, direct, handles},
    },
};

use crate::{
    api::AgentKind,
    instances::window::ShellAsks,
    moontasks::{
        ColumnEnd, ColumnId,
        store::{TaskResource, TaskResourceKind},
    },
};

/// A pid nothing is running under: higher than any pid a system hands out.
const PID_OF_NOTHING: u32 = 4_194_303;

/// A repo with a board of two cards, `Fix the races` and `Bing bong`, and their ids.
fn board(name: &str) -> (PathBuf, String, String) {
    let repo = std::env::temp_dir().join(format!(
        "moontasks-wire-cli-{}-{name}-{}",
        std::process::id(),
        store::new_uuid()
    ));
    std::fs::create_dir_all(&repo).expect("failed to create the test repo");
    let card = |title: &str| {
        store::create_task(&repo, title, &ColumnId::new("todo"), ColumnEnd::Top)
            .expect("expected the card to be made")
    };
    let (races, bing_bong) = (card("Fix the races"), card("Bing bong"));
    (repo, races, bing_bong)
}

/// The folder a shell of this task is told it is in.
fn task_dir(repo: &Path, task_id: &str) -> Option<OsString> {
    Some(
        store::task_dir(repo, task_id)
            .expect("expected the task's folder")
            .into_os_string(),
    )
}

/// Write an agent run on a task, in a shell of this id held by this process.
fn run_an_agent(repo: &Path, task_id: &str, terminal_id: &str, held_by: u32) {
    let mut metadata = store::read_task(repo, task_id).expect("expected the task");
    metadata.resources.push(TaskResource {
        id: store::new_uuid(),
        kind: TaskResourceKind::Agent,
        agent: AgentKind::Claude,
        file_path: None,
        terminal_id: Some(terminal_id.to_string()),
        terminal_owner: Some(held_by),
        agent_session_id: None,
        name: None,
        started_at_unix: 0,
        started_by: None,
        work_tree: None,
        started_by_hook: None,
    });
    store::write_task(repo, task_id, &metadata).expect("expected the run to be written");
}

fn kept(repo: &Path) -> String {
    std::fs::read_to_string(broadcasts::file_path(repo)).unwrap_or_default()
}

fn tags(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| tag.to_string()).collect()
}

/// Outside a task's shell there is nobody for the line to be from, and the folder the
/// command was typed in is not asked instead.
#[test]
fn a_post_from_outside_a_tasks_shell_is_refused() {
    for unset in [None, Some(OsString::new())] {
        let error = post(&[], "rewriting src/cli", unset).expect_err("expected a refusal");
        assert!(
            error.to_string().contains("is run from a task's shell")
                && error.to_string().contains(TASK_DIR_ENV_VAR),
            "{error}"
        );
    }
}

#[test]
fn a_post_from_a_folder_that_is_no_task_of_the_board_is_refused() {
    let (repo, _, _) = board("no-task");
    let stray = store::tasks_root(&repo).join("no-such-task");

    let error = post(&[], "rewriting src/cli", Some(stray.into_os_string()))
        .expect_err("expected a refusal");

    assert!(
        error
            .to_string()
            .contains("no-such-task is not a task of the board"),
        "{error}"
    );
    assert_eq!(kept(&repo), "");
}

#[test]
fn a_line_with_no_tags_is_posted_to_the_boards_file_signed_with_the_handle() {
    let (repo, races, bing_bong) = board("broadcast");

    post(&[], "rewriting src/terminal.rs", task_dir(&repo, &races))
        .expect("expected the first post");
    post(&[], "in src/cli", task_dir(&repo, &bing_bong)).expect("expected the second post");

    let kept = kept(&repo);
    let lines: Vec<&str> = kept.lines().collect();
    assert_eq!(lines.len(), 2, "got {kept}");
    assert!(
        lines[0].ends_with(" @fix-the-races: rewriting src/terminal.rs"),
        "got {kept}"
    );
    assert!(lines[1].ends_with(" @bing-bong: in src/cli"), "got {kept}");
}

#[test]
fn a_line_break_in_the_line_is_refused() {
    let (repo, races, _) = board("line-break");

    let error = post(&[], "one\ntwo", task_dir(&repo, &races)).expect_err("expected a refusal");

    assert!(error.to_string().contains("line break"), "{error}");
    assert_eq!(kept(&repo), "");
}

/// A tag that names nobody stops the whole post, saying who there is to tag: the other
/// tasks with an agent running, and not the cards nobody is working on. A tagged line is
/// never written to the file.
#[test]
fn a_tag_that_names_no_task_sends_nothing_and_lists_the_agents_running() {
    let (repo, races, bing_bong) = board("unknown-tag");
    let idle =
        store::create_task(&repo, "Nobody home", &ColumnId::new("done"), ColumnEnd::Top)
            .expect("expected the card to be made");
    let asks = ShellAsks::listen("moon tasks".to_string(), true, egui::Context::default())
        .expect("expected a socket");
    asks.on_project(&repo.display().to_string())
        .expect("expected the record to be written");
    run_an_agent(&repo, &races, "terminal-a", std::process::id());
    run_an_agent(&repo, &bing_bong, "terminal-b", std::process::id());
    // A run whose moon has gone is not running.
    run_an_agent(&repo, &idle, "terminal-c", PID_OF_NOTHING);

    let error = post(
        &tags(&["bing-bong", "nobody"]),
        "hello",
        task_dir(&repo, &races),
    )
    .expect_err("expected a refusal");

    assert_eq!(
        error.to_string(),
        "@nobody is no task of this board. Its tasks with an agent running: @bing-bong"
    );
    assert!(
        asks.drain_wired().is_empty(),
        "nothing is sent on a bad tag"
    );
    assert_eq!(kept(&repo), "");
}

#[test]
fn a_tagged_task_with_no_running_agent_is_refused() {
    let (repo, races, bing_bong) = board("nobody-running");
    // A run whose moon is gone is not running.
    run_an_agent(&repo, &bing_bong, "terminal-a", PID_OF_NOTHING);

    let error = post(&tags(&["bing-bong"]), "hello", task_dir(&repo, &races))
        .expect_err("expected a refusal");

    assert_eq!(error.to_string(), "@bing-bong has no running agent");
}

/// An agent whose shell is held by a moon that listens on nothing - this test process -
/// is one nobody can be asked to type into, and the post says which agent was not sent
/// the line and which moon said nothing.
#[test]
fn an_agent_held_by_a_moon_that_does_not_answer_is_refused_with_the_reason() {
    let (repo, races, bing_bong) = board("no-answer");
    run_an_agent(&repo, &bing_bong, "terminal-a", std::process::id());

    let error = post(&tags(&["bing-bong"]), "hello", task_dir(&repo, &races))
        .expect_err("expected a refusal");

    let said = format!("{error:#}");
    assert!(
        said.contains("@bing-bong was not sent the line")
            && said.contains(&format!("(process {}) did not answer", std::process::id())),
        "{said}"
    );
}

/// The whole of a direct message, over the real socket: every running agent of every
/// tagged task is sent the line once, by the window holding its shell, in its parts.
#[test]
fn a_tagged_line_reaches_the_window_holding_each_agents_shell() {
    let (repo, races, bing_bong) = board("direct");
    let asks = ShellAsks::listen("moon tasks".to_string(), true, egui::Context::default())
        .expect("expected a socket");
    asks.on_project(&repo.display().to_string())
        .expect("expected the record to be written");
    run_an_agent(&repo, &bing_bong, "terminal-a", std::process::id());
    run_an_agent(&repo, &bing_bong, "terminal-b", std::process::id());

    post(
        &tags(&["bing-bong", bing_bong.as_str()]),
        "are you in src/cli?",
        task_dir(&repo, &races),
    )
    .expect("expected the line to be sent");

    let wired = asks.drain_wired();
    assert_eq!(
        wired
            .iter()
            .map(|line| line.terminal_id.as_str())
            .collect::<Vec<_>>(),
        ["terminal-a", "terminal-b"],
        "a task tagged twice is told once, in each of its agents' shells"
    );
    for line in &wired {
        assert_eq!(line.sender, "fix-the-races");
        assert_eq!(line.recipient, "bing-bong");
        assert_eq!(line.message, "are you in src/cli?");
    }
    assert_eq!(kept(&repo), "", "a tagged line is not posted to the file");
}
