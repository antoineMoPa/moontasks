//! A hook going off: the run it starts, with its prompt filled in and handed to the agent.

use std::path::Path;

use anyhow::{Context, Result};

use super::{ColumnHook, HOOK_RUNS_IN_A_ROW_CAP, ReviewHook, Schedule};
use crate::{
    api::{AgentKind, AppState},
    moontasks::{
        reviews::{ReviewFile, describe_pull_request, reviews_root},
        service::{HookStart, start_hook_run},
        store,
    },
};

/// What a column hook's prompt is filled in from: the card that arrived, and where from.
pub(super) struct Arrival<'a> {
    pub(super) task_id: &'a str,
    /// The heading of the column it was in before, which `{from}` reads.
    pub(super) from_label: &'a str,
    /// The heading of the column it arrived in, which the run is marked with.
    pub(super) column_label: &'a str,
}

/// What became of a column hook as a card arrived.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Fired {
    Started,
    /// Hooks have started [`HOOK_RUNS_IN_A_ROW_CAP`] runs on the card with no person moving it
    /// in between, and this one was not started.
    StoppedForLoops,
}

/// Start a column's hook on a card that has just arrived in it.
pub(super) fn fire_column_hook(
    state: &AppState,
    session_id: &str,
    repo_path: &Path,
    hook: &ColumnHook,
    arrival: &Arrival,
) -> Result<Fired> {
    let mut metadata = store::read_task(repo_path, arrival.task_id)?;
    if metadata.hook_runs_in_a_row >= HOOK_RUNS_IN_A_ROW_CAP {
        return Ok(Fired::StoppedForLoops);
    }
    // Counted before the run starts, so the run's own move of the card cannot overtake it.
    metadata.hook_runs_in_a_row += 1;
    store::write_task(repo_path, arrival.task_id, &metadata)?;

    let task_dir = store::task_dir(repo_path, arrival.task_id)?;
    let prompt = fill_column_hook_prompt(
        &hook.prompt,
        &ColumnHookFillings {
            title: &metadata.title,
            notes: &store::read_notes(repo_path, arrival.task_id),
            from: arrival.from_label,
            folder: &task_dir.display().to_string(),
            id: arrival.task_id,
        },
    );
    start_with_prompt(
        state,
        session_id,
        arrival.task_id,
        hook.start,
        arrival.column_label,
        &prompt,
        None,
    )?;
    Ok(Fired::Started)
}

/// Start a review status's hook on a review that has just arrived in it: on the review's card,
/// or on the board's own task for a review that names none. Loops are stopped the way a
/// column's are, by the card's count of hook runs in a row.
pub(super) fn fire_review_hook(
    state: &AppState,
    session_id: &str,
    repo_path: &Path,
    hook: &ReviewHook,
    review: &ReviewFile,
    from: &str,
) -> Result<Fired> {
    let on_a_card = review
        .task
        .as_deref()
        .filter(|task| store::has_task_record(repo_path, task).unwrap_or(false));
    let (task_id, title, folder) = match on_a_card {
        Some(task_id) => {
            let mut metadata = store::read_task(repo_path, task_id)?;
            if metadata.hook_runs_in_a_row >= HOOK_RUNS_IN_A_ROW_CAP {
                return Ok(Fired::StoppedForLoops);
            }
            metadata.hook_runs_in_a_row += 1;
            store::write_task(repo_path, task_id, &metadata)?;
            let folder = store::task_dir(repo_path, task_id)?.display().to_string();
            (task_id.to_string(), metadata.title, folder)
        }
        None => {
            if !store::has_task_record(repo_path, store::BOARD_TASK_ID)? {
                store::create_board_task(repo_path)?;
            }
            (store::BOARD_TASK_ID.to_string(), String::new(), String::new())
        }
    };
    let file = reviews_root(repo_path)
        .join(&review.status)
        .join(format!("{}.md", review.id))
        .display()
        .to_string();
    let repo = match review.path_under_repo.as_str() {
        "" => ".",
        path => path,
    };
    let subject = review
        .suggestion
        .as_ref()
        .map(|suggestion| suggestion.subject.as_str())
        .unwrap_or_default();
    let pr = review.pr.as_deref().unwrap_or_default();
    let prompt = fill_placeholders(
        &hook.prompt,
        &[
            ("{title}", &title),
            ("{repo}", repo),
            ("{branch}", review.branch.as_deref().unwrap_or_default()),
            ("{pr}", pr),
            ("{subject}", subject),
            ("{file}", &file),
            ("{from}", from),
            ("{folder}", &folder),
            ("{id}", &review.id),
        ],
    );
    let label = format!(
        "{} {}",
        review.status,
        describe_pull_request(review.pr.as_deref(), review.branch.as_deref())
    );
    start_with_prompt(
        state,
        session_id,
        &task_id,
        hook.start,
        label.trim(),
        &prompt,
        Some(&review.id),
    )?;
    Ok(Fired::Started)
}

/// Start a schedule's run, on the board's own task.
pub(super) fn fire_schedule(
    state: &AppState,
    session_id: &str,
    repo_path: &Path,
    schedule: &Schedule,
    date: &str,
) -> Result<()> {
    if !store::has_task_record(repo_path, store::BOARD_TASK_ID)? {
        store::create_board_task(repo_path)?;
    }
    let prompt = schedule.prompt.replace("{date}", date);
    start_with_prompt(
        state,
        session_id,
        store::BOARD_TASK_ID,
        schedule.start,
        &schedule.name,
        &prompt,
        None,
    )
}

/// Start the run, and hand it the prompt: typed into a shell as its commands, or pasted into
/// an agent once it is up and reading - see [`crate::terminal::TerminalRegistry::tell_paste`].
fn start_with_prompt(
    state: &AppState,
    session_id: &str,
    task_id: &str,
    start: AgentKind,
    label: &str,
    prompt: &str,
    for_review: Option<&str>,
) -> Result<()> {
    let shell_commands =
        (start == AgentKind::None && !prompt.trim().is_empty()).then(|| prompt.to_string());
    let terminal_id = start_hook_run(
        state,
        session_id,
        task_id,
        start,
        HookStart {
            label: label.to_string(),
            shell_commands,
            for_review: for_review.map(str::to_string),
        },
    )
    .with_context(|| format!("starting {label}'s run"))?;
    if start != AgentKind::None && !prompt.trim().is_empty() {
        state
            .terminals
            .tell_paste(&terminal_id, prompt, Default::default(), Box::new(|_| {}))
            .with_context(|| format!("telling {label}'s run its prompt"))?;
    }
    Ok(())
}

/// What a column hook's placeholders stand for.
pub(super) struct ColumnHookFillings<'a> {
    pub(super) title: &'a str,
    pub(super) notes: &'a str,
    pub(super) from: &'a str,
    pub(super) folder: &'a str,
    pub(super) id: &'a str,
}

/// The prompt with `{title}`, `{notes}`, `{from}`, `{folder}` and `{id}` filled in. Filled in
/// one pass, so a card's notes holding `{title}` are not filled in again.
pub(super) fn fill_column_hook_prompt(prompt: &str, fillings: &ColumnHookFillings) -> String {
    fill_placeholders(
        prompt,
        &[
            ("{title}", fillings.title),
            ("{notes}", fillings.notes),
            ("{from}", fillings.from),
            ("{folder}", fillings.folder),
            ("{id}", fillings.id),
        ],
    )
}

/// The prompt with each placeholder filled in, in one pass - so a value holding a placeholder
/// is not filled in again.
fn fill_placeholders(prompt: &str, fillings: &[(&str, &str)]) -> String {
    let mut filled = String::with_capacity(prompt.len());
    let mut rest = prompt;
    while let Some(open) = rest.find('{') {
        filled.push_str(&rest[..open]);
        let after = &rest[open..];
        let replaced = fillings
            .iter()
            .copied()
            .find(|(placeholder, _)| after.starts_with(placeholder));
        match replaced {
            Some((placeholder, value)) => {
                filled.push_str(value.trim_end());
                rest = &after[placeholder.len()..];
            }
            None => {
                filled.push('{');
                rest = &after[1..];
            }
        }
    }
    filled.push_str(rest);
    filled
}
