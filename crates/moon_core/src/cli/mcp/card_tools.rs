//! The tools that read and change the cards: the list, one task read whole, a card made, and
//! a card changed.
//!
//! What comes back is text an agent reads, short: a task is said by its handle, which is
//! what every other tool takes.

use std::fs;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::board::{Board, HANDLE_MARK, NamedTask};
use crate::{
    cli::tasks,
    moontasks::{
        ColumnEnd, appears_on, service,
        store::{self, BoardColumn, TaskMetadata},
        wire::{direct, handles},
    },
};

/// The file a task's review request is written in - see `request_review.md` in a task's folder.
const REVIEW_REQUEST_FILE_NAME: &str = "request_for_review.txt";

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListedTasks {
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub matching: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// How many cards of a column are listed when nobody says: a column of hundreds is more than
/// an agent wants to read to find one.
const CARDS_LISTED_OF_A_COLUMN: usize = 50;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ReadTask {
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CreatedTask {
    pub title: String,
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub appears_on: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UpdatedTask {
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub remove_tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub append_notes: Option<String>,
    #[serde(default)]
    pub tracker_url: Option<String>,
}

/// Every column of the board by the name it is shown under, and under each its cards from
/// the top, a line each: the handle, the title, the tags, and whether an agent is on it.
///
/// A board keeps its finished cards, hundreds of them, and an agent asking what there is to
/// do does not want them: the column that finishes a task is counted rather than listed,
/// unless it is the column asked for or the list is narrowed by a tag or by a title.
pub fn list_tasks(board: &Board, listed: &ListedTasks) -> Result<String> {
    let repo_path = &board.repo_path;
    let folders = board.list_task_folders()?;
    let today = appears_on::today_here();
    let columns = service::cards_by_column(repo_path)?;
    let only_column = match &listed.column {
        Some(name) => {
            let shown: Vec<BoardColumn> =
                columns.iter().map(|(column, _)| column.clone()).collect();
            Some(tasks::column_called(&shown, name)?.id.clone())
        }
        None => None,
    };
    let finished_column = store::read_board(repo_path).role(store::RELEASES_SHELLS_IN);
    let tag = listed.tag.as_deref().and_then(store::tag_of);
    let matching = listed.matching.as_ref().map(|text| text.to_lowercase());
    let is_narrowed = tag.is_some() || matching.is_some();
    let limit = listed.limit.unwrap_or(CARDS_LISTED_OF_A_COLUMN).max(1);

    let mut text = String::new();
    for (column, cards) in columns {
        if only_column.as_ref().is_some_and(|only| *only != column.id) {
            continue;
        }
        let cards: Vec<&(String, TaskMetadata)> = cards
            .iter()
            .filter(|(_, card)| tag.as_ref().is_none_or(|tag| card.tags.contains(tag)))
            .filter(|(_, card)| {
                matching
                    .as_ref()
                    .is_none_or(|text| card.title.to_lowercase().contains(text))
            })
            .collect();
        let is_only_counted =
            Some(&column.id) == finished_column.as_ref() && only_column.is_none() && !is_narrowed;
        if is_only_counted && !cards.is_empty() {
            text.push_str(&format!(
                "{}  ({}, not listed: ask for column {:?})\n",
                column.label,
                count_cards(cards.len()),
                column.label
            ));
            continue;
        }
        text.push_str(&format!("{}\n", column.label));
        for (task_id, card) in cards.iter().take(limit) {
            text.push_str(&describe_card_line(task_id, card, &folders, today));
        }
        if cards.len() > limit {
            text.push_str(&format!(
                "  ... and {} more: narrow the list with `tag` or `matching`, or raise `limit`\n",
                count_cards(cards.len() - limit)
            ));
        }
    }
    Ok(text)
}

/// `1 card`, `2 cards`.
fn count_cards(count: usize) -> String {
    match count {
        1 => "1 card".to_string(),
        _ => format!("{count} cards"),
    }
}

/// One card of the list: `  @fix-the-races  Fix the races  #bug  (agent running)`.
fn describe_card_line(
    task_id: &str,
    card: &TaskMetadata,
    folders: &[String],
    today: time::Date,
) -> String {
    let mut line = format!(
        "  {HANDLE_MARK}{}  {}",
        handles::handle_of(task_id, folders),
        card.title
    );
    for tag in &card.tags {
        line.push_str(&format!("  #{tag}"));
    }
    if !direct::running_agents(card).is_empty() {
        line.push_str("  (agent running)");
    }
    if let Some(day) = card.appears_on
        && appears_on::still_to_come(Some(day), today)
    {
        line.push_str(&format!("  (appears on {day})"));
    }
    line.push('\n');
    line
}

/// Everything written on one task, the notes last since they are the long part.
pub fn read_task(board: &Board, read: &ReadTask) -> Result<String> {
    let repo_path = &board.repo_path;
    let task = board.resolve_task_named(read.task.as_deref())?;
    let folders = board.list_task_folders()?;
    let card = store::read_task(repo_path, &task.id)?;
    let folder = store::task_dir(repo_path, &task.id)?;
    let handle_of =
        |task_id: &str| format!("{HANDLE_MARK}{}", handles::handle_of(task_id, &folders));

    let mut text = format!("{HANDLE_MARK}{}  {}\n", task.handle, card.title);
    let column = match &card.status {
        Some(id) => store::read_board(repo_path)
            .columns
            .into_iter()
            .find(|column| column.id == *id)
            .map_or_else(|| id.to_string(), |column| column.label),
        None => "none: this is the board's own task".to_string(),
    };
    text.push_str(&format!("column: {column}\n"));
    if !card.tags.is_empty() {
        text.push_str(&format!("tags: {}\n", card.tags.join(", ")));
    }
    if !card.remote_task_tracker_url.is_empty() {
        text.push_str(&format!("tracker: {}\n", card.remote_task_tracker_url));
    }
    if let Some(day) = card.appears_on {
        text.push_str(&format!("appears on: {day}\n"));
    }
    text.push_str(&format!("folder: {}\n", folder.display()));

    let links = store::parents_by_link(repo_path)?;
    if let Some(parent) = links.get(&task.id) {
        text.push_str(&format!("sub-task of: {}\n", handle_of(parent)));
    }
    let mut sub_tasks: Vec<String> = links
        .iter()
        .filter(|(_, parent)| **parent == task.id)
        .map(|(child, _)| handle_of(child))
        .collect();
    sub_tasks.sort();
    if !sub_tasks.is_empty() {
        text.push_str(&format!("sub-tasks: {}\n", sub_tasks.join(", ")));
    }

    let running: Vec<String> = direct::running_agents(&card)
        .into_iter()
        .map(|agent| {
            card.resources
                .iter()
                .find(|resource| resource.terminal_id.as_deref() == Some(&agent.terminal_id))
                .and_then(|resource| resource.name.clone())
                .unwrap_or(agent.terminal_id)
        })
        .collect();
    if !running.is_empty() {
        text.push_str(&format!("agents running: {}\n", running.join(", ")));
    }

    let attached = store::read_attachments(repo_path, &task.id);
    if !attached.is_empty() {
        text.push_str(&format!("attachments: {}\n", attached.join(", ")));
    }
    let files = store::read_folder_files(repo_path, &task.id, &attached);
    if !files.is_empty() {
        text.push_str(&format!("files: {}\n", files.join(", ")));
    }
    if let Ok(asked) = fs::read_to_string(folder.join(REVIEW_REQUEST_FILE_NAME))
        && !asked.trim().is_empty()
    {
        text.push_str(&format!("\nreview request:\n{}\n", asked.trim_end()));
    }

    let notes = store::read_notes(repo_path, &task.id);
    match notes.trim().is_empty() {
        true => text.push_str("\nnotes: none\n"),
        false => text.push_str(&format!("\nnotes:\n{}\n", notes.trim_end())),
    }
    Ok(text)
}

/// Make a card and say its handle and its folder, which is what an agent works from.
pub fn create_task(board: &Board, created: &CreatedTask) -> Result<String> {
    let repo_path = &board.repo_path;
    let title = created.title.trim();
    if title.is_empty() {
        bail!("a task needs a title");
    }
    let columns = store::read_board(repo_path).columns;
    let column = match &created.column {
        Some(name) => tasks::column_called(&columns, name)?,
        None => columns
            .first()
            .context("the board has no columns to put a card in")?,
    };
    let day = created
        .appears_on
        .as_deref()
        .map(appears_on::parse_day)
        .transpose()
        // What is wrong with the day is said once: the parser's own reason under it says
        // the same thing again, twice.
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    let task_id =
        store::create_task_appearing_on(repo_path, title, &column.id, ColumnEnd::Top, day)?;
    if !created.tags.is_empty() {
        service::set_tags_in_repo(repo_path, &task_id, &created.tags)?;
    }
    if let Some(notes) = created.notes.as_deref().filter(|notes| !notes.is_empty()) {
        store::write_notes(repo_path, &task_id, notes)?;
    }
    let handle = handles::handle_of(&task_id, &board.list_task_folders()?);
    Ok(format!(
        "made {HANDLE_MARK}{handle} in {}\nfolder: {}\n",
        column.label,
        store::task_dir(repo_path, &task_id)?.display()
    ))
}

/// Change what a call asked to have changed on a task, and say each thing that was done.
///
/// The move comes last: a card moved to the column that finishes it is let go of, and what
/// was to be written on it is written first.
pub fn update_task(board: &Board, updated: &UpdatedTask) -> Result<String> {
    let repo_path = &board.repo_path;
    if updated.notes.is_some() && updated.append_notes.is_some() {
        bail!("`notes` replaces the notes and `append_notes` adds to them: ask for one");
    }
    let task = board.resolve_task_named(updated.task.as_deref())?;
    let handle = format!("{HANDLE_MARK}{}", task.handle);
    let mut done = String::new();

    if let Some(title) = &updated.title {
        service::rename_task_in_repo(repo_path, &task.id, title)?;
        done.push_str(&format!("renamed {handle} to {:?}\n", title.trim()));
    }
    if let Some(tags) = collect_tags_asked_for(repo_path, &task, updated)? {
        service::set_tags_in_repo(repo_path, &task.id, &tags)?;
        let now = store::read_task(repo_path, &task.id)?.tags;
        done.push_str(&match now.is_empty() {
            true => format!("{handle} has no tags\n"),
            false => format!("{handle} is tagged {}\n", now.join(", ")),
        });
    }
    if let Some(notes) = &updated.notes {
        store::write_notes(repo_path, &task.id, notes)?;
        done.push_str(&format!("wrote the notes of {handle}\n"));
    }
    if let Some(text) = &updated.append_notes {
        service::append_notes_in_repo(repo_path, &task.id, text)?;
        done.push_str(&format!("added to the notes of {handle}\n"));
    }
    if let Some(url) = &updated.tracker_url {
        service::set_remote_tracker_url_in_repo(repo_path, &task.id, url)?;
        done.push_str(&match url.trim().is_empty() {
            true => format!("took the tracker link off {handle}\n"),
            false => format!("linked {handle} to {}\n", url.trim()),
        });
    }
    if let Some(column) = &updated.column {
        done.push_str(&tasks::move_card_of_task(repo_path, &task.id, column)?);
    }

    if done.is_empty() {
        bail!(
            "nothing to change on {handle}: name a title, a column, tags, notes or a tracker_url"
        );
    }
    Ok(done)
}

/// The tags a card is to have once a call's `tags`, `add_tags` and `remove_tags` are read
/// against the ones it has - or nothing, for a call that said nothing about tags.
fn collect_tags_asked_for(
    repo_path: &std::path::Path,
    task: &NamedTask,
    updated: &UpdatedTask,
) -> Result<Option<Vec<String>>> {
    if updated.tags.is_none() && updated.add_tags.is_empty() && updated.remove_tags.is_empty() {
        return Ok(None);
    }
    let mut tags = match &updated.tags {
        Some(tags) => tags.clone(),
        None => store::read_task(repo_path, &task.id)?.tags,
    };
    tags.extend(updated.add_tags.iter().cloned());
    let removed = store::tags_of(updated.remove_tags.iter().map(String::as_str));
    let tags = store::tags_of(tags.iter().map(String::as_str))
        .into_iter()
        .filter(|tag| !removed.contains(tag))
        .collect();
    Ok(Some(tags))
}
