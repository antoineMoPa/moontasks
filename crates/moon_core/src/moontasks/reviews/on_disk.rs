//! The review files themselves: reading the folders into reviews, and moving a file from one
//! folder to another.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::{DEFAULT_REVIEW_STATUSES, REVIEWS_DIR_NAME};
use crate::{
    commit_suggestion::CommitSuggestion,
    moontasks::{hooks::split_front_matter, store},
};

/// The folder the review files are in, one folder per status.
pub fn reviews_root(repo_path: &Path) -> PathBuf {
    store::tasks_root(repo_path).join(REVIEWS_DIR_NAME)
}

/// Make a folder for each of the default statuses, so a board shows where reviews go.
pub fn make_review_status_folders(repo_path: &Path) -> Result<()> {
    let root = reviews_root(repo_path);
    for status in DEFAULT_REVIEW_STATUSES {
        let dir = root.join(status);
        std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    }
    Ok(())
}

/// One review file, as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewFile {
    /// The file's name without `.md`.
    pub id: String,
    /// The folder it is in.
    pub status: String,
    /// The card it was asked for on, if it names one.
    pub task: Option<String>,
    /// The repo, from the board's repo: no slash at either end, and empty for the board's own.
    pub path_under_repo: String,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub deploy: Option<String>,
    pub order: Option<i64>,
    /// A commit to review instead of the working tree - work already committed.
    pub commit: Option<String>,
    /// The commit message after the fences, if it has one.
    pub suggestion: Option<CommitSuggestion>,
    /// When the file was last written, in seconds since the epoch.
    pub written_at_unix: u64,
    /// When the file last arrived where it is - a move changes this, writing to it does too. What
    /// tells one arrival in a folder from the next, so a review moved out and back is seen
    /// arriving again - see [`crate::moontasks::hooks`].
    pub arrived_at: i64,
}

/// The statuses of the board: the default ones, then any other folder made beside them, by
/// name.
pub fn list_review_statuses(repo_path: &Path) -> Vec<String> {
    let mut statuses: Vec<String> = DEFAULT_REVIEW_STATUSES
        .iter()
        .map(|status| status.to_string())
        .collect();
    let mut extra: Vec<String> = std::fs::read_dir(reviews_root(repo_path))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| !name.starts_with('.') && !statuses.contains(name))
                .collect()
        })
        .unwrap_or_default();
    extra.sort();
    statuses.extend(extra);
    statuses
}

/// Every review file of the board, status by status in the lane's order, and within a status
/// by the order the files give, then oldest first.
///
/// A file that cannot be read is passed over rather than failing the rest: it is written by
/// agents and by hand, and the others are still worth drawing.
pub fn read_review_files(repo_path: &Path) -> Vec<ReviewFile> {
    let root = reviews_root(repo_path);
    let mut reviews = Vec::new();
    for status in list_review_statuses(repo_path) {
        let Ok(entries) = std::fs::read_dir(root.join(&status)) else {
            continue;
        };
        let mut in_status: Vec<ReviewFile> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != "md" || !path.is_file() {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_string();
                if id.starts_with('.') {
                    return None;
                }
                let text = std::fs::read_to_string(&path).ok()?;
                let metadata = std::fs::metadata(&path).ok()?;
                let mut review = parse_review_file(&id, &status, &text).ok()?;
                review.written_at_unix = modified_unix(&metadata);
                review.arrived_at = arrived_at_of(&metadata);
                Some(review)
            })
            .collect();
        in_status.sort_by(|a, b| {
            (a.order.is_none(), a.order, a.written_at_unix, &a.id).cmp(&(
                b.order.is_none(),
                b.order,
                b.written_at_unix,
                &b.id,
            ))
        });
        reviews.extend(in_status);
    }
    reviews
}

fn modified_unix(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}

/// The file's change time, which a rename moves on - unlike the time it was last written.
#[cfg(unix)]
fn arrived_at_of(metadata: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    metadata.ctime()
}

#[cfg(not(unix))]
fn arrived_at_of(metadata: &std::fs::Metadata) -> i64 {
    modified_unix(metadata) as i64
}

/// Read one file. The keys are read loosely, the way a hook's are; one the board does not know
/// is refused, so a typo does not quietly lose what it meant to say.
pub fn parse_review_file(id: &str, status: &str, text: &str) -> Result<ReviewFile> {
    let (mut keys, body) = split_front_matter(text)?;
    let mut take = |key: &str| keys.remove(key).filter(|value| !value.is_empty());
    let task = take("task");
    let path_under_repo = path_under_repo_of(take("repo").as_deref().unwrap_or("."));
    let branch = take("branch").map(|branch| branch.trim_start_matches('#').to_string());
    let pr = take("pr").map(|pr| pr.trim_start_matches('#').to_string());
    let deploy = take("deploy");
    let commit = take("commit");
    let order = match take("order") {
        Some(order) => Some(
            order
                .parse()
                .map_err(|_| anyhow!("order: {order:?} is not a number"))?,
        ),
        None => None,
    };
    if let Some(key) = keys.keys().next() {
        bail!("`{key}:` is not something a review file says");
    }
    Ok(ReviewFile {
        id: id.to_string(),
        status: status.to_string(),
        task,
        path_under_repo,
        branch,
        pr,
        deploy,
        order,
        commit,
        suggestion: suggestion_of(&body),
        written_at_unix: 0,
        arrived_at: 0,
    })
}

/// `repos/x/`, `./repos/x`, `.` - as a path under the board's repo, empty for the board's own.
fn path_under_repo_of(written: &str) -> String {
    let under = written.trim().trim_matches('/').trim();
    if under.is_empty() || under == "." {
        return String::new();
    }
    under.trim_start_matches("./").to_string()
}

/// The commit message: its first line the subject, the rest the paragraph.
fn suggestion_of(body: &str) -> Option<CommitSuggestion> {
    let body = body.trim();
    let (subject, rest) = body.split_once('\n').unwrap_or((body, ""));
    let subject = subject.trim();
    (!subject.is_empty()).then(|| CommitSuggestion {
        subject: subject.to_string(),
        paragraph: rest.trim().to_string(),
    })
}

/// Where a review's file is now, by its id - whichever folder it is in.
pub fn find_review_file(repo_path: &Path, id: &str) -> Result<PathBuf> {
    ensure_plain_name(id)?;
    let root = reviews_root(repo_path);
    list_review_statuses(repo_path)
        .into_iter()
        .map(|status| root.join(status).join(format!("{id}.md")))
        .find(|path| path.is_file())
        .ok_or_else(|| anyhow!("there is no review called {id}"))
}

/// Move a review to another status, or - with `None` - take its file away.
pub fn move_review_file(repo_path: &Path, id: &str, to_status: Option<&str>) -> Result<()> {
    let from = find_review_file(repo_path, id)?;
    let Some(status) = to_status else {
        return std::fs::remove_file(&from)
            .with_context(|| format!("removing {}", from.display()));
    };
    ensure_plain_name(status)?;
    let dir = reviews_root(repo_path).join(status);
    let to = dir.join(format!("{id}.md"));
    if to == from {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    std::fs::rename(&from, &to)
        .with_context(|| format!("moving {} to {}", from.display(), to.display()))
}

/// A review's id and status are a file's and a folder's names, so neither can climb out of the
/// folder or hide.
fn ensure_plain_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        bail!("{name:?} cannot name a review or a status");
    }
    Ok(())
}
