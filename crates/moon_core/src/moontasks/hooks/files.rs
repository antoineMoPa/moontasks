//! The hook files: a few `key: value` lines between `---` fences, and the prompt after them.
//!
//! ```text
//! ---
//! every: weekdays
//! at: 08:30
//! start: claude
//! ---
//! Read the board and say what moved since yesterday.
//! ```
//!
//! Read loosely - keys in any order, spaces around the colon, a missing closing fence - and
//! written one way, so a hook saved from the board reads the same as one written by hand.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};

use super::{
    BoardHooks, COLUMN_HOOKS_DIR_NAME, ColumnHook, HOOKS_DIR_NAME, SCHEDULES_DIR_NAME, Schedule,
    ScheduleEvery, TimeOfDay, start_from_word, start_word,
};
use crate::moontasks::{ColumnId, store};

const FENCE: &str = "---";

/// The board's hooks folder.
pub(super) fn hooks_dir(repo_path: &Path) -> PathBuf {
    store::tasks_root(repo_path).join(HOOKS_DIR_NAME)
}

fn column_hooks_dir(repo_path: &Path) -> PathBuf {
    hooks_dir(repo_path).join(COLUMN_HOOKS_DIR_NAME)
}

fn schedules_dir(repo_path: &Path) -> PathBuf {
    hooks_dir(repo_path).join(SCHEDULES_DIR_NAME)
}

pub(super) fn column_hook_path(repo_path: &Path, column: &ColumnId) -> Result<PathBuf> {
    ensure_plain_file_name(column.as_str())?;
    Ok(column_hooks_dir(repo_path).join(format!("{column}.md")))
}

pub(super) fn schedule_path(repo_path: &Path, name: &str) -> Result<PathBuf> {
    ensure_plain_file_name(name)?;
    Ok(schedules_dir(repo_path).join(format!("{name}.md")))
}

/// A hook's name is a file's name, so it cannot climb out of its folder or hide.
fn ensure_plain_file_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        bail!("{name:?} cannot name a hook file");
    }
    Ok(())
}

/// Every hook on the board, and a line for each file that could not be read.
pub(super) fn read_board_hooks(repo_path: &Path) -> BoardHooks {
    let mut hooks = BoardHooks::default();
    for (stem, path) in markdown_files_in(&column_hooks_dir(repo_path)) {
        match read_to_string(&path).and_then(|text| parse_column_hook(&ColumnId::new(stem), &text))
        {
            Ok(hook) => hooks.columns.push(hook),
            Err(error) => hooks.unreadable.push(unreadable_line(&path, &error)),
        }
    }
    for (stem, path) in markdown_files_in(&schedules_dir(repo_path)) {
        match read_to_string(&path).and_then(|text| parse_schedule(&stem, &text)) {
            Ok(schedule) => hooks.schedules.push(schedule),
            Err(error) => hooks.unreadable.push(unreadable_line(&path, &error)),
        }
    }
    hooks.schedules.sort_by(|a, b| a.name.cmp(&b.name));
    hooks
}

fn unreadable_line(path: &Path, error: &anyhow::Error) -> String {
    let shown = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{shown}: {error:#}")
}

fn read_to_string(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

/// The `.md` files of a folder, by the name before `.md`, sorted. A folder that is not there
/// has none.
fn markdown_files_in(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "md" || !path.is_file() {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_string();
            (!stem.starts_with('.')).then_some((stem, path))
        })
        .collect();
    files.sort();
    files
}

/// Whether the board has any hook files of either kind - the runner does nothing on a board
/// with none.
pub(super) fn board_has_column_hooks(repo_path: &Path) -> bool {
    !markdown_files_in(&column_hooks_dir(repo_path)).is_empty()
}

pub(super) fn board_has_schedules(repo_path: &Path) -> bool {
    !markdown_files_in(&schedules_dir(repo_path)).is_empty()
}

pub(super) fn write_column_hook(repo_path: &Path, hook: &ColumnHook) -> Result<()> {
    let path = column_hook_path(repo_path, &hook.column)?;
    write_file(
        &path,
        &[("start", start_word(hook.start).to_string())],
        &hook.prompt,
    )
}

pub(super) fn write_schedule(repo_path: &Path, schedule: &Schedule) -> Result<()> {
    let path = schedule_path(repo_path, &schedule.name)?;
    write_file(
        &path,
        &[
            ("every", schedule.every.word().to_string()),
            ("at", schedule.at.written_for(schedule.every)),
            ("start", start_word(schedule.start).to_string()),
        ]
        .into_iter()
        // Every minute has no time to say.
        .filter(|(_, value)| !value.is_empty())
        .collect::<Vec<_>>(),
        &schedule.prompt,
    )
}

fn write_file(path: &Path, keys: &[(&str, String)], prompt: &str) -> Result<()> {
    let mut text = format!("{FENCE}\n");
    for (key, value) in keys {
        text.push_str(&format!("{key}: {value}\n"));
    }
    text.push_str(&format!("{FENCE}\n"));
    text.push_str(prompt.trim_end());
    text.push('\n');
    let dir = path.parent().expect("a hook file is in a folder");
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// Take a hook's file away. One that is not there is already gone.
pub(super) fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("removing {}", path.display())),
    }
}

pub(super) fn parse_column_hook(column: &ColumnId, text: &str) -> Result<ColumnHook> {
    let (mut keys, prompt) = split_front_matter(text)?;
    let start = take_start(&mut keys)?;
    refuse_leftover_keys(keys)?;
    Ok(ColumnHook {
        column: column.clone(),
        start,
        prompt,
    })
}

pub(super) fn parse_schedule(name: &str, text: &str) -> Result<Schedule> {
    let (mut keys, prompt) = split_front_matter(text)?;
    let every = keys.remove("every").unwrap_or_else(|| "day".to_string());
    let every = ScheduleEvery::from_word(&every)
        .ok_or_else(|| anyhow!("every: {every:?} is not `minute`, `hour`, `day` or `weekdays`"))?;
    let at = match (every.asks_for(), keys.remove("at")) {
        (super::TimeAsked::Nothing, _) => TimeOfDay { hour: 0, minute: 0 },
        (super::TimeAsked::MinutePast, None) => TimeOfDay { hour: 0, minute: 0 },
        (super::TimeAsked::TimeOfDay, None) => {
            bail!("no `at:` time, such as `at: 08:30`")
        }
        (_, Some(at)) => TimeOfDay::parse_for(every, &at).ok_or_else(|| match every {
            ScheduleEvery::Hour => anyhow!("at: {at:?} is not a minute past the hour, such as :30"),
            _ => anyhow!("at: {at:?} is not a time such as 08:30"),
        })?,
    };
    let start = take_start(&mut keys)?;
    refuse_leftover_keys(keys)?;
    Ok(Schedule {
        name: name.to_string(),
        every,
        at,
        start,
        prompt,
    })
}

fn take_start(keys: &mut BTreeMap<String, String>) -> Result<crate::api::AgentKind> {
    let start = keys.remove("start").unwrap_or_else(|| "claude".to_string());
    start_from_word(&start).ok_or_else(|| {
        let words: Vec<&str> = super::HOOK_STARTS.iter().map(|(word, _)| *word).collect();
        anyhow!("start: {start:?} is not one of {}", words.join(", "))
    })
}

fn refuse_leftover_keys(keys: BTreeMap<String, String>) -> Result<()> {
    match keys.keys().next() {
        Some(key) => bail!("`{key}:` is not something a hook says"),
        None => Ok(()),
    }
}

/// The keys between the fences, and the prompt after them. A file with no fences is all
/// prompt, with every key left to its default.
fn split_front_matter(text: &str) -> Result<(BTreeMap<String, String>, String)> {
    let mut lines = text.lines();
    let first = lines.clone().next().map(str::trim);
    if first != Some(FENCE) {
        return Ok((BTreeMap::new(), text.trim().to_string()));
    }
    lines.next();
    let mut keys = BTreeMap::new();
    let mut prompt_lines: Vec<&str> = Vec::new();
    let mut in_keys = true;
    for line in lines {
        if in_keys {
            let trimmed = line.trim();
            if trimmed == FENCE {
                in_keys = false;
            } else if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            } else {
                let (key, value) = trimmed
                    .split_once(':')
                    .ok_or_else(|| anyhow!("{trimmed:?} is not a `key: value` line"))?;
                // A comment after the value, the way the docs show one.
                let value = value.split(" #").next().unwrap_or_default();
                keys.insert(key.trim().to_string(), value.trim().to_string());
            }
        } else {
            prompt_lines.push(line);
        }
    }
    Ok((keys, prompt_lines.join("\n").trim().to_string()))
}
