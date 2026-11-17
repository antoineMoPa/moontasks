//! Moving reviews along with git: to `committed/` once their commit is made, to `pushed/` once a
//! remote branch has it - and back to `committed/` if the remote loses it again.
//!
//! A review's commit is the one its `commit:` line names, or - for a review asked for before
//! anything was committed - the recent commit on its branch made with its subject. That commit
//! is then written into the file as `commit:`, so the review keeps knowing it once the branch
//! has moved on, and the lane opens what was committed.
//!
//! `requested/` is never gone back to, and `deployed/` and folders made by hand are never
//! touched: those are where a person or an agent put the review.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{
    COMMITTED_REVIEW_STATUS, FIRST_REVIEW_STATUS, OLD_REVIEW_STATUSES, PUSHED_REVIEW_STATUS,
    ReviewFile, locate_review_file_in_status, move_review_file_at, read_review_files,
    reviews_root,
};
use crate::moontasks::review_request::{COMMITTED_LOOKBACK, find_working_copy_of_review};

/// Move each review that git says is further along to that status, and the files of folders
/// older boards had to the folder that replaced them. Returns a line for each move, for the
/// hooks log.
pub fn move_reviews_to_their_git_stage(repo_path: &Path) -> Vec<String> {
    let mut said = move_files_out_of_old_review_folders(repo_path);
    for review in read_review_files(repo_path) {
        match move_review_to_its_git_stage(repo_path, &review) {
            Ok(Some(to)) => said.push(format!("review {}: {} -> {to}", review.id, review.status)),
            Ok(None) => {}
            Err(error) => said.push(format!("review {}: {error:#}", review.id)),
        }
    }
    said
}

/// Where git says one review is, when that is somewhere it should be moved to.
fn move_review_to_its_git_stage(repo_path: &Path, review: &ReviewFile) -> Result<Option<String>> {
    let follows_git = [
        FIRST_REVIEW_STATUS,
        COMMITTED_REVIEW_STATUS,
        PUSHED_REVIEW_STATUS,
    ]
    .contains(&review.status.as_str());
    if !follows_git {
        return Ok(None);
    }
    let copy =
        find_working_copy_of_review(repo_path, &review.path_under_repo, review.branch.as_deref());
    let found = match review.commit.as_deref() {
        Some(commit) => resolve_commit(&copy, commit),
        None => find_recent_commit_with_subject(&copy, review),
    };
    let Some(commit) = found else {
        return Ok(None);
    };
    // Its own folder's file: a review of the same name in another status is another review.
    let file = locate_review_file_in_status(repo_path, &review.status, &review.id)?;
    if review.commit.is_none() {
        write_commit_into_review_file(&file, &commit)?;
    }
    let to = match remote_branch_has_commit(&copy, &commit) {
        true => PUSHED_REVIEW_STATUS,
        false => COMMITTED_REVIEW_STATUS,
    };
    if to == review.status {
        return Ok(None);
    }
    move_review_file_at(repo_path, &file, &review.id, Some(to))?;
    Ok(Some(to.to_string()))
}

/// The full hash of a commit a file names, if the repo has it.
fn resolve_commit(copy: &Path, commit: &str) -> Option<String> {
    let asked = format!("{commit}^{{commit}}");
    crate::git::run_git(copy, &["rev-parse", "--verify", "--quiet", &asked])
        .ok()
        .map(|hash| hash.trim().to_string())
        .filter(|hash| !hash.is_empty())
}

/// The last commit on the review's branch made with its subject, among the last few.
fn find_recent_commit_with_subject(copy: &Path, review: &ReviewFile) -> Option<String> {
    let subject = &review.suggestion.as_ref()?.subject;
    let log = crate::git::run_git(
        copy,
        &["log", "-n", COMMITTED_LOOKBACK, "--format=%H%x1f%s", "HEAD"],
    )
    .ok()?;
    log.lines().find_map(|line| {
        let (hash, line_subject) = line.split_once('\u{1f}')?;
        (line_subject == subject).then(|| hash.to_string())
    })
}

/// Whether a remote-tracking branch has the commit - as the last fetch or push left them.
fn remote_branch_has_commit(copy: &Path, commit: &str) -> bool {
    crate::git::run_git(
        copy,
        &[
            "for-each-ref",
            "--count=1",
            "--format=%(refname)",
            "--contains",
            commit,
            "refs/remotes",
        ],
    )
    .is_ok_and(|refs| !refs.trim().is_empty())
}

/// Add `commit: <hash>` to a review file's keys.
fn write_commit_into_review_file(path: &Path, commit: &str) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.first().map(|line| line.trim()) != Some("---") {
        bail!("{} has no `---` fences", path.display());
    }
    let Some(closing) = lines.iter().skip(1).position(|line| line.trim() == "---") else {
        bail!("{} has no closing `---`", path.display());
    };
    let line = format!("commit: {commit}\n");
    lines.insert(closing + 1, &line);
    std::fs::write(path, lines.concat()).with_context(|| format!("writing {}", path.display()))
}

/// Move the files of `reviewing/` and `done/` - an older board's folders - to the status that
/// replaced each, and take the emptied folder away.
fn move_files_out_of_old_review_folders(repo_path: &Path) -> Vec<String> {
    let root = reviews_root(repo_path);
    let mut said = Vec::new();
    for (old, new) in OLD_REVIEW_STATUSES {
        let dir = root.join(old);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let _ = std::fs::create_dir_all(root.join(new));
        for entry in entries.flatten() {
            let to = root.join(new).join(entry.file_name());
            if to.exists() {
                continue;
            }
            match std::fs::rename(entry.path(), &to) {
                Ok(()) => said.push(format!(
                    "review {}: {old} is now {new}",
                    entry.file_name().to_string_lossy()
                )),
                Err(error) => said.push(format!(
                    "could not move {}: {error}",
                    entry.path().display()
                )),
            }
        }
        let _ = std::fs::remove_dir(&dir);
    }
    said
}
