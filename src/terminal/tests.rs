//! The shells' tests that need the window's side: its backend and its fixtures.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::api::AgentKind;

use super::*;

/// Renaming a task's run writes the name on the task, where it outlives the shell: the card
/// reads it after the shell is gone, and a resumed run takes it back.
#[cfg(unix)]
#[test]
fn renaming_a_tasks_run_writes_the_name_on_the_task() {
    use crate::backend::Backend;

    const TASK: &str = "write-the-parser-1111";
    let fixture = crate::native::ui_tests::seeded_fixture("run-rename");
    let state = crate::server::build_state(Arc::new(Mutex::new(Instant::now())));
    let registry = Arc::clone(&state.terminals);
    let backend = crate::backend::local::LocalBackend::new(state.clone());
    let opened = backend
        .open_session(crate::api::OpenSessionRequest {
            repo_path: fixture.root.display().to_string(),
            diff_target: None,
            active_commit: None,
        })
        .expect("expected the session to open");
    // The repo as the session has it, which is what the board keeps a task's shells under.
    let repo_path = crate::api::with_session(&state, &opened.session_id, |session| {
        Ok(session.repo_path.clone())
    })
    .expect("expected the session's repo");

    let path = fake_claude("#!/bin/sh\nsleep 30\n");
    let terminal_id = registry
        .spawn(TerminalSpec {
            cwd: fixture.root.clone(),
            program: TerminalProgram::Agent(AgentKind::Claude),
            args: Vec::new(),
            env: vec![("PATH".to_string(), path.display().to_string())],
            owner: Some(crate::moontasks::store::run_owner(&repo_path, TASK)),
            name: Some("claude - 1".to_string()),
            type_ahead: None,
            runs_as: None,
        })
        .expect("expected a shell");
    fixture.write(
        &format!(".moontasks/{TASK}/metadata.json"),
        &format!(
            "{{\n  \"title\": \"Write the parser\",\n  \"status\": \"todo\",\n  \
             \"created_at_unix\": 1700000000,\n  \"resources\": [{{\n    \"id\": \"run\",\n    \
             \"kind\": \"agent\",\n    \"agent\": \"claude\",\n    \
             \"terminal_id\": \"{terminal_id}\",\n    \"name\": \"claude - 1\",\n    \
             \"started_at_unix\": 1700000000\n  }}]\n}}\n"
        ),
    );
    let run = |backend: &crate::backend::local::LocalBackend| {
        let tasks = backend
            .list_tasks(&opened.session_id)
            .expect("expected the board");
        let task = tasks
            .into_iter()
            .find(|task| task.id == TASK)
            .expect("expected the task on the board");
        let run = &task.resources[0];
        (run.label.clone(), run.running)
    };
    assert_eq!(run(&backend), ("claude - 1".to_string(), true));

    backend
        .rename_terminal(&opened.session_id, &terminal_id, "parser")
        .expect("expected the rename");

    assert_eq!(run(&backend), ("parser".to_string(), true));
    assert_eq!(
        crate::moontasks::store::read_task(&fixture.root, TASK)
            .expect("expected the task")
            .resources[0]
            .name
            .as_deref(),
        Some("parser"),
        "the name is written on the task"
    );

    registry.remove(&terminal_id);
    assert_eq!(
        run(&backend),
        ("parser".to_string(), false),
        "and the card reads it after the shell is gone"
    );
}

/// OpenCode is told which task it is on through its environment, and this is that reaching
/// the process it was meant for: a card starts an agent, and the config OpenCode reads names
/// the `brief.md` that card just wrote.
///
/// Ignored because it starts a real OpenCode - run it with `--ignored` on a machine that has
/// one. Everything about it that can be checked without one is in
/// `moontasks::service::tests`.
#[test]
#[ignore]
fn opencode_is_started_with_a_config_naming_this_task_s_brief() {
    use crate::{
        api::OpenSessionRequest,
        backend::{Backend, local::LocalBackend},
        moontasks::{
            ColumnEnd, ColumnId, CreateTaskRequest, StartFolder, StartResourceRequest,
            TaskResourceKind,
        },
    };

    let repo = std::env::temp_dir().join("moontasks-opencode-brief");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).expect("failed to create the fixture directory");
    crate::git::run_git_no_output(&repo, &["init"]).expect("failed to init the fixture repo");

    let state = crate::server::build_state(Arc::new(Mutex::new(Instant::now())));
    let terminals = Arc::clone(&state.terminals);
    let backend = LocalBackend::new(state);
    let opened = backend
        .open_session(OpenSessionRequest {
            repo_path: repo.display().to_string(),
            diff_target: None,
            active_commit: None,
        })
        .expect("expected the session to open");
    let task = backend
        .create_task(
            &opened.session_id,
            &CreateTaskRequest {
                title: "Repaint the retro encabulator".to_string(),
                status: ColumnId::new("todo"),
                joins: ColumnEnd::Top,
            },
        )
        .expect("expected the task to be created");
    let terminal_id = backend
        .start_task_resource(
            &opened.session_id,
            &task.id,
            StartResourceRequest {
                kind: TaskResourceKind::Agent,
                agent: crate::api::AgentKind::OpenCode,
                opens_in: StartFolder::Repo,
                unattended: false,
            },
        )
        .expect("expected OpenCode to start");
    let session = terminals.get(&terminal_id).expect("expected the shell");

    // What the child was actually given, read off the process rather than taken on trust.
    std::thread::sleep(Duration::from_secs(3));
    let pid = session
        .child_pid
        .expect("expected the agent to have a process");
    let environment = std::process::Command::new("ps")
        .args(["eww", "-p", &pid.to_string()])
        .output()
        .expect("expected to read the process environment");
    let environment = String::from_utf8_lossy(&environment.stdout).to_string();
    terminals.remove(&terminal_id);

    let carried = environment
        .split_whitespace()
        .find(|word| word.starts_with("OPENCODE_CONFIG_CONTENT="))
        .unwrap_or_else(|| panic!("OpenCode was started with no config: {environment}"));

    let brief_path = std::path::Path::new(&task.dir_path).join("brief.md");
    assert!(
        carried.contains(&brief_path.display().to_string()),
        "the config should name this task's brief, got {carried}"
    );
    let brief = std::fs::read_to_string(&brief_path)
        .expect("the brief is written before the agent is started");
    assert!(
        brief.contains("Repaint the retro encabulator"),
        "and the file it names has to be the brief: {brief}"
    );
}

/// A folder holding a fake `claude` for the PATH, so an agent shell can be spawned
/// without the real agent - the spec's own env wins over the PATH the spawn sets.
#[cfg(unix)]
fn fake_claude(script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!(
        "moontasks-fake-agent-{}",
        crate::moontasks::store::new_uuid()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create the fake agent's folder");
    let path = dir.join("claude");
    std::fs::write(&path, script).expect("failed to write the fake agent");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("failed to make the fake agent executable");
    dir
}
