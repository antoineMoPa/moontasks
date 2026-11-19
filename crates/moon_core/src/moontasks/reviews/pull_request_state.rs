//! Whether a review's pull request is open, merged or closed, as the forge says - what the
//! review's row on its card shows beside the pull request's number.
//!
//! Asked of `gh`, a few pull requests at each look and never from the thread that draws: the
//! answers are kept here for the rows to read. One that is merged or closed is not asked about
//! again; an open one is, every few minutes, and so is one `gh` could not answer for.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

use super::ReviewFile;
use crate::moontasks::review_request::find_working_copy_of_review;

/// How long an answer that can still change - open, or none - stands before `gh` is asked again.
const ASKED_AGAIN_AFTER_SECS: u64 = 300;
/// How many pull requests one look asks about, so a board with many does not hold the look up.
const ASKED_PER_LOOK: usize = 3;

pub const OPEN_PULL_REQUEST: &str = "open";
pub const MERGED_PULL_REQUEST: &str = "merged";
pub const CLOSED_PULL_REQUEST: &str = "closed";

struct Answer {
    /// `None` when `gh` had no answer: missing, signed out, or no such pull request.
    state: Option<&'static str>,
    asked_at: Instant,
}

impl Answer {
    fn can_still_change(&self) -> bool {
        !matches!(
            self.state,
            Some(MERGED_PULL_REQUEST) | Some(CLOSED_PULL_REQUEST)
        )
    }
}

/// The answers so far, by working copy and the `pr:` the review file gives.
static ANSWERS: Mutex<Option<HashMap<(PathBuf, String), Answer>>> = Mutex::new(None);

/// The state last heard of a pull request, or `None` when it has not been heard yet.
pub fn read_known_pull_request_state(working_copy: &Path, pr: &str) -> Option<String> {
    let guard = ANSWERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .as_ref()?
        .get(&(working_copy.to_path_buf(), pr.to_string()))?
        .state
        .map(str::to_string)
}

/// Ask `gh` about the pull requests of these reviews whose state is not known or may have
/// changed - up to [`ASKED_PER_LOOK`] of them.
pub(super) fn refresh_pull_request_states(repo_path: &Path, reviews: &[ReviewFile]) {
    let mut asked = 0;
    for review in reviews {
        let Some(pr) = review.pr.as_deref() else {
            continue;
        };
        let copy = find_working_copy_of_review(
            repo_path,
            &review.path_under_repo,
            review.branch.as_deref(),
        );
        let key = (copy, pr.to_string());
        if !is_pull_request_due_to_be_asked(&key) {
            continue;
        }
        let state = ask_forge_for_pull_request_state(&key.0, pr);
        let mut guard = ANSWERS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.get_or_insert_with(HashMap::new).insert(
            key,
            Answer {
                state,
                asked_at: Instant::now(),
            },
        );
        asked += 1;
        if asked == ASKED_PER_LOOK {
            return;
        }
    }
}

fn is_pull_request_due_to_be_asked(key: &(PathBuf, String)) -> bool {
    let guard = ANSWERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match guard.as_ref().and_then(|answers| answers.get(key)) {
        None => true,
        Some(answer) => {
            answer.can_still_change()
                && answer.asked_at.elapsed().as_secs() >= ASKED_AGAIN_AFTER_SECS
        }
    }
}

/// What `gh` says of one pull request. Not asked in a repo with no `origin`, which has no forge
/// to ask.
fn ask_forge_for_pull_request_state(working_copy: &Path, pr: &str) -> Option<&'static str> {
    crate::git::run_git(working_copy, &["remote", "get-url", "origin"]).ok()?;
    let output = std::process::Command::new("gh")
        .args(["pr", "view", pr, "--json", "state", "--jq", ".state"])
        .current_dir(working_copy)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pull_request_state(&String::from_utf8_lossy(&output.stdout))
}

/// `OPEN`, `MERGED` or `CLOSED`, as `gh` prints them.
fn parse_pull_request_state(said: &str) -> Option<&'static str> {
    match said.trim().to_ascii_lowercase().as_str() {
        "open" => Some(OPEN_PULL_REQUEST),
        "merged" => Some(MERGED_PULL_REQUEST),
        "closed" => Some(CLOSED_PULL_REQUEST),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forges_answer_is_read_whatever_its_case_and_nothing_else_is() {
        assert_eq!(parse_pull_request_state("MERGED\n"), Some("merged"));
        assert_eq!(parse_pull_request_state("open"), Some("open"));
        assert_eq!(parse_pull_request_state("CLOSED"), Some("closed"));
        assert_eq!(parse_pull_request_state(""), None);
        assert_eq!(parse_pull_request_state("no pull requests found"), None);
    }

    #[test]
    fn an_open_pull_request_is_asked_about_again_and_a_merged_one_is_not() {
        let open = Answer {
            state: Some(OPEN_PULL_REQUEST),
            asked_at: Instant::now(),
        };
        let unanswered = Answer {
            state: None,
            asked_at: Instant::now(),
        };
        let merged = Answer {
            state: Some(MERGED_PULL_REQUEST),
            asked_at: Instant::now(),
        };
        assert!(open.can_still_change());
        assert!(unanswered.can_still_change());
        assert!(!merged.can_still_change());
    }
}
