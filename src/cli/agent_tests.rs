//! `moon agent`'s tests that need a window to answer them - see
//! [`moon_core::cli::agent`].

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use moon_core::cli::agent::*;
#[allow(unused_imports)]
use std::path::Path;
#[allow(unused_imports)]
use anyhow::{Context, Result, bail};
#[allow(unused_imports)]
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

use crate::{
    instances::{AgentAsks, window::ShellAsks},
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
        "moontasks-agent-cli-{}-{name}-{}",
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

/// Write an agent run on a task, in a shell of this id held by this process.
fn run_an_agent(repo: &Path, task_id: &str, terminal_id: &str, name: &str, held_by: u32) {
    let mut metadata = store::read_task(repo, task_id).expect("expected the task");
    metadata.resources.push(TaskResource {
        id: store::new_uuid(),
        kind: TaskResourceKind::Agent,
        agent: AgentKind::Claude,
        file_path: None,
        terminal_id: Some(terminal_id.to_string()),
        terminal_owner: Some(held_by),
        agent_session_id: None,
        name: Some(name.to_string()),
        started_at_unix: 0,
        started_by: None,
        work_tree: None,
        started_by_hook: None,
        for_review: None,
    });
    store::write_task(repo, task_id, &metadata).expect("expected the run to be written");
}

/// A window's answers about agents, written down as they are asked for.
#[derive(Default)]
struct Asked {
    started: Mutex<Vec<(String, String, AgentKind)>>,
    told: Mutex<Vec<(String, String)>>,
}

impl AgentAsks for Asked {
    fn start(&self, repo_path: &str, task_id: &str, agent: AgentKind) -> Result<String> {
        if agent == AgentKind::Codex {
            bail!("Codex is not installed here");
        }
        self.started
            .lock()
            .unwrap()
            .push((repo_path.to_string(), task_id.to_string(), agent));
        Ok("fix the races claude - 1".to_string())
    }

    fn tell(&self, terminal_id: &str, line: &str) -> Result<()> {
        self.told
            .lock()
            .unwrap()
            .push((terminal_id.to_string(), line.to_string()));
        Ok(())
    }

    fn shown(&self, terminal_id: &str, wanted: Shown) -> Result<String> {
        Ok(format!("{wanted:?} of {terminal_id}\n"))
    }
}

/// A window open on this repo, answering about agents, and what it is asked.
fn window_on(repo: &Path) -> (ShellAsks, Arc<Asked>) {
    let asks = ShellAsks::listen("moon tasks".to_string(), true, egui::Context::default())
        .expect("expected a socket");
    asks.on_project(&repo.display().to_string())
        .expect("expected the record to be written");
    let asked = Arc::new(Asked::default());
    asks.agents_answered_by(Arc::clone(&asked) as Arc<dyn AgentAsks>);
    (asks, asked)
}

/// The whole of a start, over the real socket: the window open on the board's repo is
/// asked for the agent on the task the handle names, and what it called the run is said.
#[test]
fn an_agent_is_started_by_the_window_open_on_the_boards_repo() {
    let (repo, races, _) = board("start");
    let (_asks, asked) = window_on(&repo);

    let said = start(&repo, "fix-the-races", AgentKind::Claude).expect("expected a start");

    assert_eq!(said, "started fix the races claude - 1 on @fix-the-races\n");
    assert_eq!(
        *asked.started.lock().unwrap(),
        [(repo.display().to_string(), races, AgentKind::Claude)]
    );
}

#[test]
fn a_start_with_no_window_on_the_repo_is_refused_with_how_to_open_one() {
    let (repo, _, _) = board("no-window");
    let (elsewhere, _, _) = board("elsewhere");
    // A window on another project is not one to start this board's agents in.
    let (_asks, asked) = window_on(&elsewhere);

    let error =
        start(&repo, "fix-the-races", AgentKind::Claude).expect_err("expected a refusal");

    assert!(
        error
            .to_string()
            .contains(&format!("no moon window is open on {}", repo.display()))
            && error.to_string().contains("`moon tasks`"),
        "{error}"
    );
    assert!(asked.started.lock().unwrap().is_empty());
}

/// What the window could not do is what the command says, in the window's words.
#[test]
fn a_start_the_window_refuses_says_why() {
    let (repo, _, _) = board("refused");
    let (_asks, _asked) = window_on(&repo);

    let error =
        start(&repo, "fix-the-races", AgentKind::Codex).expect_err("expected a refusal");

    assert!(
        error.to_string().contains("Codex is not installed here"),
        "{error}"
    );
}

#[test]
fn a_handle_that_names_no_task_is_refused_with_the_board_it_was_looked_for_on() {
    let (repo, _, _) = board("unknown");

    let error = start(&repo, "nobody", AgentKind::Claude).expect_err("expected a refusal");

    assert_eq!(
        error.to_string(),
        format!(
            "@nobody is no task of this board, {}",
            store::tasks_root(&repo).display()
        )
    );
}

/// The whole of a tell, over the real socket: the line reaches the window holding the
/// shell of each of the task's running agents, as it was written.
#[test]
fn a_line_is_told_to_each_running_agent_of_the_task_as_it_was_written() {
    let (repo, races, bing_bong) = board("tell");
    let (_asks, asked) = window_on(&repo);
    let this_process = std::process::id();
    run_an_agent(
        &repo,
        &races,
        "terminal-a",
        "fix the races claude - 1",
        this_process,
    );
    run_an_agent(
        &repo,
        &races,
        "terminal-b",
        "fix the races codex - 2",
        this_process,
    );
    // A run whose moon has gone is not running, and neither is another task's told.
    run_an_agent(
        &repo,
        &races,
        "terminal-c",
        "fix the races pi - 3",
        PID_OF_NOTHING,
    );
    run_an_agent(
        &repo,
        &bing_bong,
        "terminal-d",
        "bing bong claude - 4",
        this_process,
    );

    let said = tell(&repo, "fix-the-races", "start with the tests").expect("expected a tell");

    assert_eq!(
        said,
        "told fix the races claude - 1\ntold fix the races codex - 2\n"
    );
    assert_eq!(
        *asked.told.lock().unwrap(),
        [
            ("terminal-a".to_string(), "start with the tests".to_string()),
            ("terminal-b".to_string(), "start with the tests".to_string()),
        ]
    );
}

/// Every running agent of the board, under the handle `view` and `tell` take - whoever
/// holds its shell - and none of the runs that are over.
#[test]
fn the_list_is_every_running_agent_of_the_board_by_its_tasks_handle() {
    let (repo, races, bing_bong) = board("list");
    let this_process = std::process::id();

    assert_eq!(
        list(&repo).expect("expected a list"),
        format!(
            "no agent is running on the board in {}\n",
            store::tasks_root(&repo).display()
        )
    );

    run_an_agent(
        &repo,
        &races,
        "terminal-a",
        "fix the races claude - 1",
        this_process,
    );
    // A run whose moon has gone is not running.
    run_an_agent(
        &repo,
        &races,
        "terminal-b",
        "fix the races pi - 2",
        PID_OF_NOTHING,
    );
    run_an_agent(
        &repo,
        &races,
        "terminal-c",
        "fix the races codex - 3",
        this_process,
    );
    run_an_agent(
        &repo,
        &bing_bong,
        "terminal-d",
        "bing bong claude - 4",
        this_process,
    );

    let listed = list(&repo).expect("expected a list");

    let mut lines: Vec<&str> = listed.lines().collect();
    lines.sort();
    assert_eq!(
        lines,
        [
            "@bing-bong      bing bong claude - 4",
            "@fix-the-races  fix the races claude - 1",
            "@fix-the-races  fix the races codex - 3",
        ],
        "got {listed}"
    );
}

#[test]
fn a_task_with_no_running_agent_is_refused_with_how_to_start_one() {
    let (repo, races, _) = board("nobody-running");
    run_an_agent(
        &repo,
        &races,
        "terminal-a",
        "fix the races claude - 1",
        PID_OF_NOTHING,
    );

    for error in [
        tell(&repo, "fix-the-races", "hello").expect_err("expected a refusal"),
        view(&repo, "fix-the-races", Shown::Screen).expect_err("expected a refusal"),
    ] {
        assert_eq!(
            error.to_string(),
            "@fix-the-races has no running agent: start one with `moon agent start \
             fix-the-races <agent>`"
        );
    }
}

/// An agent whose shell is held by a moon that listens on nothing - this test process -
/// is one nobody can be asked about, and the command says which run was not told and
/// which moon said nothing.
#[test]
fn an_agent_held_by_a_moon_that_does_not_answer_is_refused_with_the_reason() {
    let (repo, races, _) = board("no-answer");
    run_an_agent(
        &repo,
        &races,
        "terminal-a",
        "fix the races claude - 1",
        std::process::id(),
    );

    let error = tell(&repo, "fix-the-races", "hello").expect_err("expected a refusal");

    let said = format!("{error:#}");
    assert!(
        said.contains("fix the races claude - 1 was not told the line")
            && said.contains(&format!("(process {}) did not answer", std::process::id())),
        "{said}"
    );
}

#[test]
fn a_line_break_in_the_line_is_refused_before_anybody_is_told() {
    let (repo, races, _) = board("line-break");
    let (_asks, asked) = window_on(&repo);
    run_an_agent(
        &repo,
        &races,
        "terminal-a",
        "fix the races claude - 1",
        std::process::id(),
    );

    let error = tell(&repo, "fix-the-races", "one\ntwo").expect_err("expected a refusal");

    assert!(error.to_string().contains("line break"), "{error}");
    assert!(asked.told.lock().unwrap().is_empty());
}

/// One agent's screen is printed as it is, so it can be piped; several are told apart by
/// the names of their runs.
#[test]
fn a_view_is_what_the_window_says_each_running_agent_is_showing() {
    let (repo, races, bing_bong) = board("view");
    let (_asks, _asked) = window_on(&repo);
    let this_process = std::process::id();
    run_an_agent(
        &repo,
        &races,
        "terminal-a",
        "fix the races claude - 1",
        this_process,
    );
    run_an_agent(
        &repo,
        &bing_bong,
        "terminal-b",
        "bing bong claude - 2",
        this_process,
    );
    run_an_agent(
        &repo,
        &bing_bong,
        "terminal-c",
        "bing bong codex - 3",
        this_process,
    );

    assert_eq!(
        view(&repo, "fix-the-races", Shown::LastRows(40)).expect("expected a view"),
        "LastRows(40) of terminal-a\n"
    );
    assert_eq!(
        view(&repo, "bing-bong", Shown::Screen).expect("expected a view"),
        "== bing bong claude - 2 ==\nScreen of terminal-b\n\
         == bing bong codex - 3 ==\nScreen of terminal-c\n"
    );
}

/// The three commands against a real agent, with nothing standing in for anything: a
/// window's socket, its moon's shells, and a Claude that is started, told something and
/// read back.
///
/// Ignored because it starts a real Claude - run it with `--ignored` on a machine that
/// has one. Everything about it that can be checked without one is in the tests above
/// and in `terminal::told` and `terminal::shown`.
///
/// The board is in a repo of its own under this checkout's `target`, which Claude has
/// not been run in the first time this is: it comes up asking whether it trusts the
/// folder, and the start answers that. Claude writes the answer down, so only the first
/// run on a machine goes that way.
#[test]
#[ignore]
fn a_real_agent_is_started_told_and_viewed() {
    use std::time::{Duration, Instant};

    use crate::{
        api::OpenSessionRequest,
        backend::{Backend, local::LocalBackend},
        moontasks::CreateTaskRequest,
        native::agent_asks::WindowAgents,
    };

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("agent-from-the-command-line");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).expect("failed to create the fixture directory");
    let repo = repo.canonicalize().expect("expected the repo to resolve");
    crate::git::run_git_no_output(&repo, &["init"]).expect("failed to init the fixture repo");

    let state = crate::server::build_state(Arc::new(Mutex::new(Instant::now())));
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::new(state));
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
                title: "Sums".to_string(),
                status: ColumnId::new("todo"),
                joins: ColumnEnd::Top,
            },
        )
        .expect("expected the task to be created");
    let asks = ShellAsks::listen("moon tasks".to_string(), true, egui::Context::default())
        .expect("expected a socket");
    asks.on_project(&repo.display().to_string())
        .expect("expected the record to be written");
    asks.agents_answered_by(Arc::new(WindowAgents {
        backend: Arc::clone(&backend),
    }));

    let started = start(&repo, "sums", AgentKind::Claude).expect("expected a start");
    assert_eq!(started, "started Sums claude - 1 on @sums\n");
    let screen_showing = |wanted: &[&str]| {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let screen = view(&repo, "sums", Shown::Screen).expect("expected the screen");
            if wanted.iter().any(|wanted| screen.contains(wanted)) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "never showed {wanted:?}:\n{screen}"
            );
            std::thread::sleep(Duration::from_millis(500));
        }
    };
    // Told the moment it was started, which is before it has a box to be told in.
    let told = tell(
        &repo,
        "sums",
        "What is 123 plus 456? Answer with the number and nothing else.",
    )
    .expect("expected a tell");
    assert_eq!(told, "told Sums claude - 1\n");

    let screen = screen_showing(&["579"]);
    let last_rows = view(&repo, "sums", Shown::LastRows(200)).expect("expected the rows");
    let run = store::read_task(&repo, &task.id)
        .expect("expected the task")
        .resources
        .remove(0);
    backend
        .stop_task_resource(&opened.session_id, &task.id, &run.id)
        .expect("expected the agent to stop");

    println!("the screen:\n{screen}\nthe last rows:\n{last_rows}");
    assert!(last_rows.contains("What is 123 plus 456?"), "{last_rows}");
}
