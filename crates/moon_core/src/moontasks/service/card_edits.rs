//! What is written on a card, changed in the repo its board is in: the title, the tags, the
//! tracker link, the notes.
//!
//! By the repo rather than by a window's session, so the commands that open no window - see
//! `crate::cli::mcp` - change a card the way the board's own menus do.

use std::path::Path;

use anyhow::{Result, bail};

use crate::moontasks::{remote_tracker, store};

/// Give a task a different title. The folder keeps the name it was created with, because it
/// is what everything else - shells, agent sessions, whatever an agent wrote - points at.
pub fn rename_task_in_repo(repo_path: &Path, task_id: &str, title: &str) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        bail!("a task needs a title");
    }
    let mut metadata = store::read_task(repo_path, task_id)?;
    metadata.title = title.to_string();
    store::write_task(repo_path, task_id, &metadata)
}

/// What a card is marked with, set whole. Spelled the way the store keeps tags, so a tag typed
/// twice in two spellings lands as one.
pub fn set_tags_in_repo(repo_path: &Path, task_id: &str, tags: &[String]) -> Result<()> {
    let mut metadata = store::read_task(repo_path, task_id)?;
    metadata.tags = store::tags_of(tags.iter().map(String::as_str));
    store::write_task(repo_path, task_id, &metadata)
}

/// Link a task to its issue in a tracker kept elsewhere, or take the link off with an empty
/// one - see [`remote_tracker`].
pub fn set_remote_tracker_url_in_repo(repo_path: &Path, task_id: &str, url: &str) -> Result<()> {
    let url = url.trim();
    if !url.is_empty() && !remote_tracker::is_a_web_link(url) {
        bail!("a tracker link starts with https:// or http://, and `{url}` does not");
    }
    let mut metadata = store::read_task(repo_path, task_id)?;
    metadata.remote_task_tracker_url = url.to_string();
    store::write_task(repo_path, task_id, &metadata)
}

/// Write `text` under what a task's notes already say, on a line of its own.
pub fn append_notes_in_repo(repo_path: &Path, task_id: &str, text: &str) -> Result<()> {
    let mut notes = store::read_notes(repo_path, task_id);
    if !notes.is_empty() && !notes.ends_with('\n') {
        notes.push('\n');
    }
    notes.push_str(text);
    if !notes.ends_with('\n') {
        notes.push('\n');
    }
    store::write_notes(repo_path, task_id, &notes)
}
