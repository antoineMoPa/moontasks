//! What fires the hooks: a thread of the moon holding the board's sessions - a window, or
//! `moon serve` - that looks at each board it has open every couple of seconds.
//!
//! A card's arrival is seen as its column, or the time it entered it, being other than at the
//! last look, whoever moved it - a window, `moon tasks move`, a text editor. A card made and
//! moved between two looks is seen by having entered its column after it was made. The first
//! look at a board only takes note of where its cards are, so opening moon fires nothing for
//! the moves made while it was closed.
//!
//! Every moon with the board open looks, and the one that fires is the window last focused on
//! it - named in `.moontasks/hooks/.watcher` - so a hook's run is in a window the person is
//! using, where it can be opened. With no such window still running, any moon may fire.
//!
//! Of the moons that may fire, only one does. Before firing, a moon claims
//! the arrival or the schedule's day by making a file under `.moontasks/hooks/.fired/` that
//! must not already be there - the one whose file it was fires, and the others see it exists.

use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use anyhow::Result;

use super::{
    clock::{LocalNow, due_slot_of},
    files::{board_has_column_hooks, board_has_schedules, hooks_dir, read_board_hooks},
    firing::{Arrival, Fired, fire_column_hook, fire_schedule},
};
use crate::{
    api::AppState,
    moontasks::{ColumnId, store},
};

/// How often each open board is looked at.
const LOOKS_EVERY: Duration = Duration::from_secs(2);
/// How long a claim is kept. Long past the day a schedule's claim is for; an arrival's is
/// never asked about again once the card has moved on.
const CLAIMS_KEPT_FOR: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const CLAIMS_SWEPT_EVERY: Duration = Duration::from_secs(60 * 60);
const CLAIMS_DIR_NAME: &str = ".fired";
/// What the hooks have done and failed to do, a line each, beside them.
const LOG_FILE_NAME: &str = "hooks.log";

/// The file naming the moon whose window was last focused with the board open.
const WATCHER_FILE_NAME: &str = ".watcher";

/// Whether this moon's window has the keyboard - see [`note_window_focus`].
static WINDOW_FOCUSED: AtomicBool = AtomicBool::new(false);

/// Tell the runner whether this moon's window has the keyboard. While it has, its boards'
/// hooks fire in this moon.
pub fn note_window_focus(focused: bool) {
    WINDOW_FOCUSED.store(focused, Ordering::Relaxed);
}

/// Start looking at the boards `state` has open, for as long as the process runs.
pub fn start_hook_runner(state: AppState) {
    let spawned = std::thread::Builder::new()
        .name("moon-hooks".to_string())
        .spawn(move || {
            let mut watch = Watch::default();
            loop {
                std::thread::sleep(LOOKS_EVERY);
                look_at_open_boards(&state, &mut watch);
            }
        });
    if let Err(error) = spawned {
        eprintln!("moon: the hooks could not be started: {error}");
    }
}

/// What the runner remembers between looks.
#[derive(Default)]
struct Watch {
    /// Each board's cards, by the column each was in at the last look. A board with no column
    /// hooks is not looked at, and loses its entry - so a hook written later fires on the moves
    /// after it, not on one made before.
    columns_seen: HashMap<PathBuf, HashMap<String, CardPlace>>,
    claims_swept_at: HashMap<PathBuf, Instant>,
}

fn look_at_open_boards(state: &AppState, watch: &mut Watch) {
    for (repo_path, session_id) in open_boards(state) {
        if let Err(error) = look_at_board(state, &session_id, &repo_path, watch) {
            write_log_line(
                &repo_path,
                &format!("could not look at the board: {error:#}"),
            );
        }
    }
}

/// Each board with a session open on it, with one of those sessions: the one its runs are
/// started through.
fn open_boards(state: &AppState) -> Vec<(PathBuf, String)> {
    let sessions: Vec<(String, PathBuf)> = {
        let inner = state.inner.lock().unwrap();
        let mut sessions: Vec<(String, PathBuf)> = inner
            .sessions
            .iter()
            .map(|(id, session)| (id.clone(), session.repo_path.clone()))
            .collect();
        sessions.sort();
        sessions
    };
    let mut boards: Vec<(PathBuf, String)> = Vec::new();
    for (session_id, folder) in sessions {
        let Ok(repo_path) = store::board_checkout(&folder) else {
            continue;
        };
        if !store::tasks_root(&repo_path).is_dir() {
            continue;
        }
        if boards.iter().all(|(seen, _)| *seen != repo_path) {
            boards.push((repo_path, session_id));
        }
    }
    boards
}

fn look_at_board(
    state: &AppState,
    session_id: &str,
    repo_path: &Path,
    watch: &mut Watch,
) -> Result<()> {
    let has_column_hooks = board_has_column_hooks(repo_path);
    let has_schedules = board_has_schedules(repo_path);
    if !has_column_hooks {
        watch.columns_seen.remove(repo_path);
    }
    if !has_column_hooks && !has_schedules {
        return Ok(());
    }
    if WINDOW_FOCUSED.load(Ordering::Relaxed) {
        name_this_moon_the_watcher(repo_path);
    }
    let fires_here = !another_moon_watches(repo_path);
    let hooks = read_board_hooks(repo_path);
    if has_column_hooks {
        fire_on_arrivals(state, session_id, repo_path, &hooks, watch, fires_here)?;
    }
    if has_schedules && fires_here {
        fire_due_schedules(state, session_id, repo_path, &hooks.schedules);
    }
    sweep_old_claims(repo_path, watch);
    Ok(())
}

fn fire_on_arrivals(
    state: &AppState,
    session_id: &str,
    repo_path: &Path,
    hooks: &super::BoardHooks,
    watch: &mut Watch,
    fires_here: bool,
) -> Result<()> {
    let now = cards_by_column(repo_path)?;
    // Noted even when another moon fires, so that this one picks up from here if it takes over.
    let Some(before) = watch
        .columns_seen
        .insert(repo_path.to_path_buf(), now.clone())
    else {
        return Ok(());
    };
    if !fires_here {
        return Ok(());
    }
    let board = store::read_board(repo_path);
    let label_of = |column: &ColumnId| {
        board
            .columns
            .iter()
            .find(|known| known.id == *column)
            .map_or_else(|| column.to_string(), |known| known.label.clone())
    };

    for (task_id, place) in &now {
        let column = &place.column;
        let from_label = match before.get(task_id) {
            Some(was) if was == place => continue,
            Some(was) => label_of(&was.column),
            // A card that was not on the board at the last look was made since. It arrived
            // only if it was moved after it was made, from a column no look saw it in.
            None if place.moved_since_made => NEW_CARD_FROM.to_string(),
            None => continue,
        };
        let Some(hook) = hooks.of_column(column) else {
            continue;
        };
        let entered_at = place.entered_at;
        if !claim(
            repo_path,
            &format!("arrival-{task_id}-{column}-{entered_at}"),
        ) {
            continue;
        }
        let column_label = label_of(column);
        let arrival = Arrival {
            task_id,
            from_label: &from_label,
            column_label: &column_label,
        };
        let said = match fire_column_hook(state, session_id, repo_path, hook, &arrival) {
            Ok(Fired::Started) => format!("{column_label}: started on {task_id}"),
            Ok(Fired::StoppedForLoops) => format!(
                "{column_label}: not started on {task_id} - hooks have started {} runs on it \
                 in a row; moving it yourself starts them again",
                super::HOOK_RUNS_IN_A_ROW_CAP
            ),
            Err(error) => format!("{column_label}: failed on {task_id}: {error:#}"),
        };
        write_log_line(repo_path, &said);
    }
    Ok(())
}

/// What `{from}` says of a card made and moved between two looks.
const NEW_CARD_FROM: &str = "a new card";

/// Where a card was at a look: its column, and when it entered it - a card moved out and back
/// between two looks is in the same column, but entered it later.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CardPlace {
    column: ColumnId,
    entered_at: u64,
    moved_since_made: bool,
}

/// The place of each card, by its id. The board task is on none.
fn cards_by_column(repo_path: &Path) -> Result<HashMap<String, CardPlace>> {
    let mut columns = HashMap::new();
    for task_id in store::list_task_ids(repo_path)? {
        let Ok(metadata) = store::read_task(repo_path, &task_id) else {
            continue;
        };
        let entered_at = metadata.entered_column_at_unix.unwrap_or_default();
        if let Some(column) = metadata.status {
            columns.insert(
                task_id,
                CardPlace {
                    column,
                    entered_at,
                    moved_since_made: entered_at > metadata.created_at_unix,
                },
            );
        }
    }
    Ok(columns)
}

fn fire_due_schedules(
    state: &AppState,
    session_id: &str,
    repo_path: &Path,
    schedules: &[super::Schedule],
) {
    let now = LocalNow::read();
    for schedule in schedules {
        let Some(slot) = due_slot_of(schedule, &now) else {
            continue;
        };
        if !claim(repo_path, &schedule_claim_name(&schedule.name, &slot)) {
            continue;
        }
        let said = match fire_schedule(state, session_id, repo_path, schedule, &now.date) {
            Ok(()) => format!("{}: started", schedule.name),
            Err(error) => format!("{}: failed: {error:#}", schedule.name),
        };
        write_log_line(repo_path, &said);
    }
}

/// The claim on one slot of a schedule: its day, hour or minute.
fn schedule_claim_name(name: &str, slot: &str) -> String {
    format!("schedule-{name}-{}", slot.replace(':', "-"))
}

/// Take the slot a schedule is in now as run, without running it: one saved after its time
/// today waits for tomorrow - an hourly one for the next hour, an every-minute one for the next
/// minute - rather than going off the moment it is saved.
pub(super) fn claim_today_if_already_past(repo_path: &Path, schedule: &super::Schedule) {
    let now = LocalNow::read();
    if let Some(slot) = due_slot_of(schedule, &now) {
        claim(repo_path, &schedule_claim_name(&schedule.name, &slot));
    }
}

/// Make the claim's file, and answer whether this moon made it - which is whether it fires.
fn watcher_path(repo_path: &Path) -> PathBuf {
    hooks_dir(repo_path).join(WATCHER_FILE_NAME)
}

fn name_this_moon_the_watcher(repo_path: &Path) {
    let pid = std::process::id().to_string();
    let path = watcher_path(repo_path);
    if std::fs::read_to_string(&path).is_ok_and(|named| named.trim() == pid) {
        return;
    }
    if let Err(error) = std::fs::write(&path, &pid) {
        write_log_line(
            repo_path,
            &format!("could not name this moon the watcher: {error}"),
        );
    }
}

/// Whether the board's watcher is another moon that is still running - the one to fire.
fn another_moon_watches(repo_path: &Path) -> bool {
    std::fs::read_to_string(watcher_path(repo_path))
        .ok()
        .and_then(|named| named.trim().parse::<u32>().ok())
        .is_some_and(|pid| {
            pid != std::process::id() && crate::moontasks::service::process_is_running(pid)
        })
}

fn claim(repo_path: &Path, name: &str) -> bool {
    let dir = hooks_dir(repo_path).join(CLAIMS_DIR_NAME);
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(name))
        .is_ok()
}

fn sweep_old_claims(repo_path: &Path, watch: &mut Watch) {
    let due = watch
        .claims_swept_at
        .get(repo_path)
        .is_none_or(|at| at.elapsed() >= CLAIMS_SWEPT_EVERY);
    if !due {
        return;
    }
    watch
        .claims_swept_at
        .insert(repo_path.to_path_buf(), Instant::now());
    let Ok(entries) = std::fs::read_dir(hooks_dir(repo_path).join(CLAIMS_DIR_NAME)) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= CLAIMS_KEPT_FOR);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Add a line to the hooks' log. A hook fires with nobody at the window, so this is the only
/// account of what it did or why it did not.
fn write_log_line(repo_path: &Path, said: &str) {
    let dir = hooks_dir(repo_path);
    let _ = std::fs::create_dir_all(&dir);
    let Ok(mut log) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG_FILE_NAME))
    else {
        return;
    };
    let now = LocalNow::read();
    let minute = now.minute_of_day;
    let _ = writeln!(
        log,
        "{} {:02}:{:02} {said}",
        now.date,
        minute / 60,
        minute % 60
    );
}
