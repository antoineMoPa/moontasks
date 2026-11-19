//! Reviews as files: one pull request - or one branch to commit - per file, in a folder named
//! after where its review stands.
//!
//! ```text
//! .moontasks/reviews/
//!   requested/encabulator-races-tests.md
//!   committed/encabulator-fix-the-races.md
//!   pushed/encabulator-races-refactor.md
//!   deployed/encabulator-races-docs.md
//! ```
//!
//! `committed/` and `pushed/` follow git: a review whose commit is made moves to `committed/`
//! on its own, and to `pushed/` once a remote branch has it - see [`git_stage`]. `deployed/` is
//! moved to by hand, by a person or an agent with `mv`; nothing asks for it any more.
//!
//! A task is often several pull requests, in one repo or several, each at its own stage. A file
//! here has a status - its folder - and each is a row on its task's card, which is the only
//! place a review is shown: a review that names no task, or a task the board does not have, is
//! on no card. The reviews of a deleted task are put away with it - see
//! [`put_away_reviews_of_deleted_tasks`].
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
//! ---
//! fix(encabulator): put the bearing races back
//!
//! They were reversed, which is why the flux only ran one way.
//! ```
//!
//! Where the review and the code stand is the folder. Older files may still carry a `deploy:`
//! line, which is read and shown but moves nothing.

#[cfg(not(target_arch = "wasm32"))]
mod git_stage;
#[cfg(not(target_arch = "wasm32"))]
mod on_disk;
#[cfg(not(target_arch = "wasm32"))]
mod pull_request_link;
#[cfg(not(target_arch = "wasm32"))]
mod pull_request_state;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub use git_stage::move_reviews_to_their_git_stage;
#[cfg(not(target_arch = "wasm32"))]
pub use on_disk::*;
#[cfg(not(target_arch = "wasm32"))]
pub use pull_request_link::find_pull_request_url;
#[cfg(not(target_arch = "wasm32"))]
pub use pull_request_state::read_known_pull_request_state;

use serde::{Deserialize, Serialize};

/// The folder, under `.moontasks`, of the review files.
pub const REVIEWS_DIR_NAME: &str = "reviews";

/// The statuses every board has, in the order a review goes through them. A folder made by hand
/// beside them is a status too, after these.
pub const DEFAULT_REVIEW_STATUSES: &[&str] = &[
    FIRST_REVIEW_STATUS,
    COMMITTED_REVIEW_STATUS,
    PUSHED_REVIEW_STATUS,
    DEPLOYED_REVIEW_STATUS,
];
/// Where a new review is written.
pub const FIRST_REVIEW_STATUS: &str = "requested";
/// Where a review goes once its commit is made - on its own, see [`git_stage`].
pub const COMMITTED_REVIEW_STATUS: &str = "committed";
/// Where a review goes once a remote branch has its commit - on its own too.
pub const PUSHED_REVIEW_STATUS: &str = "pushed";
/// Where a review goes once its code is live. Only a person or an agent moves it here.
pub const DEPLOYED_REVIEW_STATUS: &str = "deployed";
/// The folders older boards had, and the status each one's files now belong in.
pub const OLD_REVIEW_STATUSES: &[(&str, &str)] = &[
    ("reviewing", FIRST_REVIEW_STATUS),
    ("done", COMMITTED_REVIEW_STATUS),
];

/// What the agent docs call the page that says how to ask for a review.
pub const REVIEWS_BRIEF_FILE_NAME: &str = "request_review.md";

/// That page. Read by an agent about to hand work over, so every word is one it needs.
pub const REVIEWS_BRIEF: &str = "\
# Requesting a review

One file per PR (or branch to commit), several per repo if needed:
`.moontasks/reviews/requested/<repo>-<branch>.md` - for the board's own repo use its folder
name, and a review of one commit can be named after its subject: `moontasks-fix-huge-images.md`.

```
---
task: <your task folder's name>
repo: repos/x        # from the board's repo; `.` for the board's own
branch: my-branch    # optional
pr: 412              # optional
order: 1             # optional: its place among your task's reviews
commit: <sha>        # optional: review a commit instead of the working tree (short or full)
---
feat(x): commit subject

Optional paragraph.
```

The body is the commit message the person will use; for a commit already made, copy its
message. The folder is the status: `requested/`, `committed/`, `pushed/`. moon moves a review
to `committed/` once its commit is made (its `commit:`, or a recent commit with its subject)
and to `pushed/` once a remote branch has it - don't move those yourself. The review shows on
your task's card, so `task:` must be your task folder's exact name. To update a request, edit
its file - a file already committed or pushed is done: new work gets a new request, under a
name no status folder has yet.
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
    /// Whether the pull request is `open`, `merged` or `closed`, once the forge has said - see
    /// `pull_request_state`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_state: Option<String>,
    /// Where the code stands - `pending`, `merged`, `deployed` - as the file wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<String>,
    /// A commit to review instead of the working tree, when the file names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Its place in its task's deploy order, when the file says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    /// The file, from the board's repo - what the dismiss mark's hover names.
    #[serde(default)]
    pub repo_path_of_file: String,
}

/// Where a review is being moved to over the wire: a status, or `None` to dismiss it - take its
/// file away.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveReviewRequest {
    /// The folder the review is in, so a namesake in another folder is never the one moved. An
    /// older window sends none, and the first folder holding that name is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_status: Option<String>,
    #[serde(default)]
    pub to_status: Option<String>,
}

/// What a review's row says the pull request is: `PR 412`, or the branch.
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
