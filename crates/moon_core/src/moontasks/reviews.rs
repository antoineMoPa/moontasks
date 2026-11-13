//! Reviews as files: one pull request - or one branch to commit - per file, in a folder named
//! after where its review stands.
//!
//! ```text
//! .moontasks/reviews/
//!   requested/encabulator-races-tests.md
//!   reviewing/encabulator-fix-the-races.md
//!   done/encabulator-races-refactor.md
//! ```
//!
//! A task is often several pull requests, in one repo or several, each at its own stage. A file
//! here has a status - its folder - and moving it to another folder is what changes it, whoever
//! moves it:
//! the board's lane, an agent, `mv` in a terminal. A hook can start on each arrival - see
//! [`crate::moontasks::hooks`].
//!
//! One file is everything about the pull request: a few `key: value` lines between `---`
//! fences, and the commit message after them.
//!
//! ```text
//! ---
//! task: fix-the-races-3c62...
//! repo: repos/retro_encabulator
//! branch: fix-the-races
//! pr: 412
//! deploy: pending
//! ---
//! fix(encabulator): put the bearing races back
//!
//! They were reversed, which is why the flux only ran one way.
//! ```
//!
//! Where the review stands is the folder; where the code stands - `deploy:` - is a line in the
//! file, written by whoever deploys it.

#[cfg(not(target_arch = "wasm32"))]
mod on_disk;
#[cfg(not(target_arch = "wasm32"))]
mod pull_request_link;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub use on_disk::*;
#[cfg(not(target_arch = "wasm32"))]
pub use pull_request_link::find_pull_request_url;

use serde::{Deserialize, Serialize};

/// The folder, under `.moontasks`, of the review files.
pub const REVIEWS_DIR_NAME: &str = "reviews";

/// The statuses every board has, in the order the lane draws them. A folder made by hand beside
/// them is a status too, drawn after these.
pub const DEFAULT_REVIEW_STATUSES: &[&str] = &["requested", "reviewing", "done"];
/// Where a new review is written.
pub const FIRST_REVIEW_STATUS: &str = "requested";
/// The status a review is finished with in. A review here is no longer pending.
pub const DONE_REVIEW_STATUS: &str = "done";

/// What the agent docs call the page that says how to ask for a review.
pub const REVIEWS_BRIEF_FILE_NAME: &str = "request_review.md";

/// That page. Read by an agent about to hand work over, so every word is one it needs.
pub const REVIEWS_BRIEF: &str = "\
# Requesting a review

One file per PR (or branch to commit), several per repo if needed:
`.moontasks/reviews/requested/<repo>-<branch>.md`

```
---
task: <your task folder's name>
repo: repos/x        # from the board's repo; `.` for the board's own
branch: my-branch    # optional
pr: 412              # optional
order: 1             # optional: deploy order within your task
commit: <sha>        # optional: review a commit instead of the working tree
---
feat(x): commit subject

Optional paragraph.
```

The body is the commit message the person will use. The folder is the status
(`requested/`, `reviewing/`, `done/`); moving the file changes it and can start a hook - leave
that to the person unless asked. Once merged or deployed, add `deploy: merged` or
`deploy: deployed`. To update a request, edit its file.
";

/// Which file a review is, and where it stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFileView {
    /// The file's name without `.md`, which is how the review is named to be moved.
    pub id: String,
    /// The folder it is in.
    pub status: String,
    /// The pull request, as the file wrote it - `412`, or a link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    /// The pull request's page, when it can be told - see `find_pull_request_url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    /// Where the code stands - `pending`, `merged`, `deployed` - as the file wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<String>,
    /// A commit to review instead of the working tree, when the file names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Its place in its task's deploy order, when the file says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    /// The file, from the board's repo - what the lane's hover shows.
    #[serde(default)]
    pub repo_path_of_file: String,
}

/// Where a review is being moved to over the wire: a status, or `None` to take it away.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveReviewRequest {
    #[serde(default)]
    pub to_status: Option<String>,
}

/// What the lane says the pull request is: `PR 412`, or the branch.
pub fn describe_pull_request(pr: Option<&str>, branch: Option<&str>) -> String {
    match (pr, branch) {
        (Some(pr), _) if pr.chars().all(|c| c.is_ascii_digit()) => format!("PR {pr}"),
        (Some(pr), _) => pr
            .rsplit('/')
            .find(|part| !part.is_empty())
            .map_or_else(|| pr.to_string(), |last| format!("PR {last}")),
        (None, Some(branch)) => format!("#{branch}"),
        (None, None) => String::new(),
    }
}
