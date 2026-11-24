use std::sync::{Arc, Mutex};

use super::{
    clock::{LocalNow, due_slot_of},
    files::{
        parse_column_hook, parse_schedule, read_board_hooks, write_column_hook, write_schedule,
    },
    firing::{Arrival, ColumnHookFillings, Fired, fill_column_hook_prompt, fire_column_hook},
    *,
};
use crate::{api::AgentKind, moontasks::store, test_fixture::Fixture};

fn standup() -> Schedule {
    Schedule {
        name: "standup".to_string(),
        every: ScheduleEvery::Weekdays,
        at: TimeOfDay {
            hour: 8,
            minute: 30,
        },
        start: AgentKind::Claude,
        harness: None,
        prompt: "Read the board.\nSay what moved since yesterday.".to_string(),
    }
}

fn now(weekend: bool, hour: u32, minute: u32) -> LocalNow {
    LocalNow {
        date: "2026-11-12".to_string(),
        weekend,
        minute_of_day: hour * 60 + minute,
    }
}

#[test]
fn a_hook_saved_from_the_board_reads_back_the_same() {
    let repo = Fixture::new("hooks-round-trip");
    let hook = ColumnHook {
        column: ColumnId::new("qa"),
        start: AgentKind::None,
        harness: None,
        prompt: "cargo test\nmoon tasks move done".to_string(),
    };
    write_column_hook(&repo.root, &hook).unwrap();
    write_schedule(&repo.root, &standup()).unwrap();

    let read = read_board_hooks(&repo.root);
    assert_eq!(read.columns, [hook]);
    assert_eq!(read.schedules, [standup()]);
    assert!(read.unreadable.is_empty(), "{:?}", read.unreadable);
    assert_eq!(
        std::fs::read_to_string(repo.root.join(schedule_repo_path("standup"))).unwrap(),
        "---\nevery: weekdays\nat: 08:30\nstart: claude\n---\nRead the board.\nSay what moved since yesterday.\n"
    );
}

/// A word after `start:` that is none of moon's own names a harness of the person's.
#[test]
fn a_hook_that_starts_a_harness_names_it_in_its_file() {
    let repo = Fixture::new("hooks-harness");
    let hook = ColumnHook {
        column: ColumnId::new("qa"),
        start: AgentKind::None,
        harness: Some("opus-plan".to_string()),
        prompt: "Plan the work.".to_string(),
    };
    write_column_hook(&repo.root, &hook).unwrap();

    assert_eq!(read_board_hooks(&repo.root).columns, [hook]);
    assert_eq!(
        std::fs::read_to_string(repo.root.join(".moontasks/hooks/columns/qa.md")).unwrap(),
        "---\nstart: opus-plan\n---\nPlan the work.\n"
    );
    assert!(
        parse_column_hook(&ColumnId::new("qa"), "---\nstart: Not A Name\n---\nx\n").is_err(),
        "a start that could name nothing should not be read"
    );
}

#[test]
fn a_hook_written_by_hand_is_read_loosely() {
    let hook = parse_column_hook(
        &ColumnId::new("qa"),
        "---\n  start :  codex   # the fast one\n\n---\n\nRun the tests.\n",
    )
    .unwrap();
    assert_eq!(hook.start, AgentKind::Codex);
    assert_eq!(hook.prompt, "Run the tests.");

    // No fences at all: the whole file is the prompt, for claude.
    let bare = parse_column_hook(&ColumnId::new("qa"), "Just the prompt.").unwrap();
    assert_eq!(bare.start, AgentKind::Claude);
    assert_eq!(bare.prompt, "Just the prompt.");

    let schedule = parse_schedule("daily", "---\nat: 9:05\n---\nhi").unwrap();
    assert_eq!(schedule.every, ScheduleEvery::Day);
    assert_eq!(schedule.at, TimeOfDay { hour: 9, minute: 5 });
}

#[test]
fn a_hook_file_that_cannot_be_read_says_why_and_the_others_still_fire() {
    let repo = Fixture::new("hooks-unreadable");
    write_schedule(&repo.root, &standup()).unwrap();
    let broken = repo.root.join(schedule_repo_path("broken"));
    std::fs::write(&broken, "---\nat: noon\n---\nx").unwrap();
    let typo = repo.root.join(column_hook_repo_path(&ColumnId::new("qa")));
    std::fs::create_dir_all(typo.parent().unwrap()).unwrap();
    std::fs::write(&typo, "---\nstrat: claude\n---\nx").unwrap();

    let read = read_board_hooks(&repo.root);
    assert_eq!(read.schedules, [standup()]);
    assert_eq!(read.unreadable.len(), 2, "{:?}", read.unreadable);
    assert!(read.unreadable.iter().any(|line| line.contains("noon")));
    assert!(read.unreadable.iter().any(|line| line.contains("strat")));
}

#[test]
fn times_of_day_are_hours_and_minutes() {
    assert_eq!(
        TimeOfDay::parse_hh_mm("08:30"),
        Some(TimeOfDay {
            hour: 8,
            minute: 30
        })
    );
    assert_eq!(
        TimeOfDay::parse_hh_mm("23:59").map(TimeOfDay::to_hh_mm),
        Some("23:59".to_string())
    );
    for wrong in ["24:00", "8:60", "8h30", "8:3", "", ":30", "830"] {
        assert_eq!(TimeOfDay::parse_hh_mm(wrong), None, "{wrong}");
    }
}

#[test]
fn a_schedule_is_due_from_its_time_on_its_days() {
    let schedule = standup();
    assert_eq!(due_slot_of(&schedule, &now(false, 8, 29)), None);
    assert_eq!(
        due_slot_of(&schedule, &now(false, 8, 30)).as_deref(),
        Some("2026-11-12")
    );
    // Missed at 08:30 - moon was closed - it still runs later that day, in the same slot.
    assert_eq!(
        due_slot_of(&schedule, &now(false, 17, 0)).as_deref(),
        Some("2026-11-12")
    );
    // Not at the weekend, for a weekday one.
    assert_eq!(due_slot_of(&schedule, &now(true, 9, 0)), None);

    let daily = Schedule {
        every: ScheduleEvery::Day,
        ..standup()
    };
    assert!(due_slot_of(&daily, &now(true, 9, 0)).is_some());
}

#[test]
fn an_hourly_schedule_runs_once_an_hour_and_an_every_minute_one_once_a_minute() {
    let hourly = Schedule {
        every: ScheduleEvery::Hour,
        at: TimeOfDay {
            hour: 0,
            minute: 15,
        },
        ..standup()
    };
    assert_eq!(due_slot_of(&hourly, &now(false, 9, 14)), None);
    assert_eq!(
        due_slot_of(&hourly, &now(false, 9, 15)).as_deref(),
        Some("2026-11-12T09")
    );
    assert_eq!(
        due_slot_of(&hourly, &now(true, 9, 59)).as_deref(),
        Some("2026-11-12T09")
    );

    let every_minute = Schedule {
        every: ScheduleEvery::Minute,
        ..standup()
    };
    assert_eq!(
        due_slot_of(&every_minute, &now(false, 9, 1)).as_deref(),
        Some("2026-11-12T09:01")
    );
}

#[test]
fn hourly_and_every_minute_schedules_are_written_with_as_much_time_as_they_need() {
    let repo = Fixture::new("hooks-hourly");
    let hourly = Schedule {
        name: "sync".to_string(),
        every: ScheduleEvery::Hour,
        at: TimeOfDay { hour: 0, minute: 5 },
        ..standup()
    };
    let every_minute = Schedule {
        name: "ping".to_string(),
        every: ScheduleEvery::Minute,
        at: TimeOfDay { hour: 0, minute: 0 },
        ..standup()
    };
    write_schedule(&repo.root, &hourly).unwrap();
    write_schedule(&repo.root, &every_minute).unwrap();
    assert_eq!(
        read_board_hooks(&repo.root).schedules,
        [every_minute.clone(), hourly.clone()]
    );
    assert!(
        std::fs::read_to_string(repo.root.join(schedule_repo_path("sync")))
            .unwrap()
            .contains("every: hour\nat: :05\n")
    );
    assert!(
        !std::fs::read_to_string(repo.root.join(schedule_repo_path("ping")))
            .unwrap()
            .contains("at:")
    );
    assert_eq!(hourly.pill_text(), "sync · hourly :05");
    assert_eq!(every_minute.pill_text(), "ping · every minute");
}

#[test]
fn a_prompt_is_filled_in_one_pass() {
    let filled = fill_column_hook_prompt(
        "Test {title} ({id}), moved from {from}.\n{notes}\n{unknown} {",
        &ColumnHookFillings {
            title: "Fix the login",
            notes: "Notes that say {title}\n",
            from: "IN PROGRESS",
            folder: "/tmp/task",
            id: "fix-the-login-1",
        },
    );
    assert_eq!(
        filled,
        "Test Fix the login (fix-the-login-1), moved from IN PROGRESS.\nNotes that say {title}\n{unknown} {"
    );
}

#[test]
fn hooks_stop_on_a_card_they_have_run_on_too_many_times_in_a_row() {
    let repo = Fixture::new("hooks-loop-cap");
    let task_id = store::create_task(
        &repo.root,
        "Loops",
        &ColumnId::new("todo"),
        store::ColumnEnd::Bottom,
    )
    .unwrap();
    let mut metadata = store::read_task(&repo.root, &task_id).unwrap();
    metadata.hook_runs_in_a_row = HOOK_RUNS_IN_A_ROW_CAP;
    store::write_task(&repo.root, &task_id, &metadata).unwrap();

    let state = crate::api::AppState::for_a_test(Arc::new(Mutex::new(std::time::Instant::now())));
    let hook = ColumnHook {
        column: ColumnId::new("todo"),
        start: AgentKind::Claude,
        harness: None,
        prompt: "again".to_string(),
    };
    let fired = fire_column_hook(
        &state,
        "no-session-needed",
        &repo.root,
        &hook,
        &Arrival {
            task_id: &task_id,
            from_label: "QA",
            column_label: "TODO",
        },
    )
    .unwrap();
    assert_eq!(fired, Fired::StoppedForLoops);
    assert!(
        store::read_task(&repo.root, &task_id)
            .unwrap()
            .resources
            .is_empty()
    );
}

#[test]
fn a_card_moved_from_the_command_line_keeps_its_count_of_hook_runs() {
    let repo = Fixture::new("hooks-moved-by-cli");
    let task_id = store::create_task(
        &repo.root,
        "Chained",
        &ColumnId::new("todo"),
        store::ColumnEnd::Bottom,
    )
    .unwrap();
    let mut metadata = store::read_task(&repo.root, &task_id).unwrap();
    metadata.hook_runs_in_a_row = 3;
    store::write_task(&repo.root, &task_id, &metadata).unwrap();

    let terminals =
        crate::terminal::TerminalRegistry::new(Arc::new(Mutex::new(std::time::Instant::now())));
    crate::moontasks::service::place_tasks_in_repo(
        &terminals,
        &repo.root,
        std::slice::from_ref(&task_id),
        ColumnId::new("in_progress"),
        0,
        None,
    )
    .unwrap();
    assert_eq!(
        store::read_task(&repo.root, &task_id)
            .unwrap()
            .hook_runs_in_a_row,
        3
    );
}

#[test]
fn run_now_without_a_card_named_takes_the_top_card_of_the_column() {
    let repo = Fixture::new("hooks-run-now-top-card");
    let todo = ColumnId::new("todo");
    assert_eq!(
        super::running_now::find_top_card_of_column(&repo.root, &todo).unwrap(),
        None
    );
    store::create_task(&repo.root, "Lower", &todo, store::ColumnEnd::Bottom).unwrap();
    let top = store::create_task(&repo.root, "Top", &todo, store::ColumnEnd::Top).unwrap();
    store::create_task(
        &repo.root,
        "Elsewhere",
        &ColumnId::new("done"),
        store::ColumnEnd::Top,
    )
    .unwrap();
    assert_eq!(
        super::running_now::find_top_card_of_column(&repo.root, &todo).unwrap(),
        Some(top)
    );
}

/// Stands in for an agent that has finished its turn: it says what Claude Code says once it is
/// waiting at its box, and stays open as an agent does.
#[cfg(unix)]
const FINISHES_AND_WAITS: &str = "#!/bin/sh\n/bin/stty -icanon\nprintf 'done\\033]9;Claude is waiting for your input\\007'\n/bin/sleep 60\n";

/// Stands in for an agent stopped on a permission.
#[cfg(unix)]
const ASKS_A_PERMISSION: &str = "#!/bin/sh\n/bin/stty -icanon\nprintf 'working\\033]9;Claude needs your permission to use Bash\\007'\n/bin/sleep 60\n";

/// Stands in for an agent somebody answered, which then finished.
#[cfg(unix)]
const FINISHES_ONCE_TYPED_AT: &str = "#!/bin/sh\n/bin/stty -icanon\nIFS= read -r line\nprintf 'done\\033]9;Claude is waiting for your input\\007'\n/bin/sleep 60\n";

/// Start a script as a hook's agent run, and wait until it asks for a person - after
/// `typed`, if anything is, has been typed into it by one.
#[cfg(unix)]
fn start_fake_hook_run(
    state: &crate::api::AppState,
    repo: &Fixture,
    script: &str,
    typed: Option<&[u8]>,
) -> String {
    let terminal_id = crate::terminal::tests::spawn_fake_claude(&state.terminals, script);
    super::ending::note_hook_agent_run(&terminal_id, repo.root.clone(), "QA", "a-card");
    if let Some(typed) = typed {
        state
            .terminals
            .get(&terminal_id)
            .unwrap()
            .write_input(typed)
            .unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline && state.terminals.attention(&terminal_id).is_none()
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        state.terminals.attention(&terminal_id).is_some(),
        "expected the run to ask for a person"
    );
    terminal_id
}

#[cfg(unix)]
#[test]
fn a_hooks_agent_run_is_closed_once_it_has_finished() {
    let repo = Fixture::new("hooks-run-closed-when-done");
    let state = crate::api::AppState::for_a_test(Arc::new(Mutex::new(std::time::Instant::now())));
    let terminal_id = start_fake_hook_run(&state, &repo, FINISHES_AND_WAITS, None);

    super::ending::close_hook_runs_that_are_done(&state);

    assert!(
        !state.terminals.is_live(&terminal_id),
        "a finished run should be closed"
    );
    let log = std::fs::read_to_string(repo.root.join(".moontasks/hooks/hooks.log")).unwrap();
    assert!(
        log.contains("QA: done on a-card"),
        "the log should say it was closed: {log}"
    );
}

#[cfg(unix)]
#[test]
fn a_hooks_agent_run_asking_a_question_or_taken_up_by_a_person_stays_open() {
    let repo = Fixture::new("hooks-run-kept-open");
    let state = crate::api::AppState::for_a_test(Arc::new(Mutex::new(std::time::Instant::now())));
    let asking = start_fake_hook_run(&state, &repo, ASKS_A_PERMISSION, None);
    let taken_up = start_fake_hook_run(&state, &repo, FINISHES_ONCE_TYPED_AT, Some(b"go on\n"));

    super::ending::close_hook_runs_that_are_done(&state);

    assert!(
        state.terminals.is_live(&asking),
        "a run waiting on a permission is not done"
    );
    assert!(
        state.terminals.is_live(&taken_up),
        "a run somebody typed into is theirs to close"
    );
}
