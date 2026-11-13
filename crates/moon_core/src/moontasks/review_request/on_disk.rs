//! Reading the board's review files into rows, each with how its repo stands in git.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::moontasks::{
    ReviewRequestView,
    reviews::{REVIEWS_DIR_NAME, ReviewFileView, read_review_files},
};

/// How far back a branch's history is read for a commit made with a review's subject: a review
/// is committed and then left while the work goes on, so its commit is one of the last few on
/// the branch - not one from months ago.
const COMMITTED_LOOKBACK: &str = "30";

/// Every review file of the board, in the lane's order, each with where its work is and how
/// much of it is uncommitted.
///
/// A repo with no board, or a board with no review, has an empty list - neither is an error.
pub fn list_for_repo(repo_path: &Path) -> Vec<ReviewRequestView> {
    // The reviews of a board mostly name the same repo on the same branch, and where that is
    // and how much is changed there is three or four git runs to find out - once per read, not
    // once per review.
    let mut reviewed: HashMap<(String, Option<String>), Reviewed> = HashMap::new();
    read_review_files(repo_path)
        .into_iter()
        .enumerate()
        .map(|(index, file)| {
            let Reviewed {
                name,
                path,
                changed_files,
                recent_subjects,
            } = reviewed
                .entry((file.path_under_repo.clone(), file.branch.clone()))
                .or_insert_with(|| {
                    reviewed_by(repo_path, &file.path_under_repo, file.branch.as_deref())
                })
                .clone();
            let committed = file
                .suggestion
                .as_ref()
                .is_some_and(|suggestion| recent_subjects.contains(&suggestion.subject));
            ReviewRequestView {
                task_id: file.task.clone().unwrap_or_default(),
                index,
                path_under_repo: file.path_under_repo.clone(),
                repo_path: path.display().to_string(),
                name,
                branch: file.branch.clone(),
                suggestion: file.suggestion.clone(),
                changed_files,
                committed,
                written_at_unix: file.written_at_unix,
                file: ReviewFileView {
                    repo_path_of_file: format!(
                        ".moontasks/{REVIEWS_DIR_NAME}/{}/{}.md",
                        file.status, file.id
                    ),
                    id: file.id,
                    status: file.status,
                    pr_url: file.pr.as_deref().and_then(|pr| {
                        crate::moontasks::reviews::find_pull_request_url(repo_path, &path, pr)
                    }),
                    pr: file.pr,
                    deploy: file.deploy,
                    order: file.order,
                    commit: file.commit,
                },
            }
        })
        .collect()
}

/// The work a review is about: where it is, what to call it, how much of it is uncommitted, and
/// the subjects of the last commits made there.
#[derive(Clone)]
struct Reviewed {
    name: String,
    path: PathBuf,
    changed_files: usize,
    /// Newest first, [`COMMITTED_LOOKBACK`] of them - what says a review's commit has been made.
    recent_subjects: Vec<String>,
}

/// Find the work a line names, which is what git is asked for a row.
///
/// The path is resolved the way the submodule hub resolves a submodule's, so a request and a hub
/// row for the same repo carry the same string and can be told to be the same repo. A path that
/// resolves to no repo is kept as it was written: the row is still worth drawing, and opening it
/// is what says what is wrong with it.
fn reviewed_by(repo_path: &Path, path_under_repo: &str, branch: Option<&str>) -> Reviewed {
    let joined = repo_path.join(path_under_repo);
    let repo = crate::git::canonicalize_repo(&joined).unwrap_or(joined);
    // The row is named after the repo, not after wherever its branch is checked out: a worktree
    // is a directory named after a task, and `pending moontasks review` is what the person
    // is looking for on the card.
    let name = repo
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| repo.display().to_string());
    let path = working_copy_of(&repo, branch);
    Reviewed {
        name,
        // A path that is not a repo at all cannot be counted, and reads as changed: the row is
        // what the agent asked for, and it should not read as dealt with because nothing could
        // be read there.
        changed_files: crate::git::changed_file_count(&path).unwrap_or(1),
        recent_subjects: recent_subjects_of(&path),
        path,
    }
}

/// The subjects of the last commits on the branch a working copy is on. None for a path that is
/// not a repo, or a repo with no commit yet: nothing has been committed there to match a line.
fn recent_subjects_of(path: &Path) -> Vec<String> {
    crate::git::run_git(
        path,
        &["log", "-n", COMMITTED_LOOKBACK, "--format=%s", "HEAD"],
    )
    .map(|log| log.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

/// Where the work a line names actually is - which is what its review, its commit and its push
/// are of.
///
/// A line naming a branch means the commit belongs on that branch, not that anyone should be
/// moved onto it. An agent working on a branch usually made a worktree to do it in, precisely so
/// that nobody's HEAD had to move; so that worktree is where the review goes, and committing and
/// pushing there are already on the right branch with nothing checked out and nothing switched.
///
/// The repo itself, when it is the thing on that branch or the line named none. And the repo
/// again when the branch is checked out nowhere - there is nothing better to offer, and the
/// commit pane is what says the branch is not the one that was asked for.
fn working_copy_of(repo: &Path, branch: Option<&str>) -> PathBuf {
    let Some(branch) = branch else {
        return repo.to_path_buf();
    };
    // Asked first, and not only as a shortcut: `git worktree list` inside a submodule names the
    // main worktree by its gitdir under `.git/modules`, which is not where its files are. Every
    // path the listing is trusted for below belongs to a linked worktree, which is a real one.
    if crate::git::current_branch_name(repo)
        .ok()
        .flatten()
        .as_deref()
        == Some(branch)
    {
        return repo.to_path_buf();
    }
    crate::git::worktree_on_branch(repo, branch)
        .ok()
        .flatten()
        .unwrap_or_else(|| repo.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A review naming a branch is reviewed where that branch is checked out.
    ///
    /// An agent working on a branch makes a worktree for it so that nobody's HEAD has to move.
    /// The line says which branch the commit belongs on; this is what turns that into the place
    /// the review, the commit and the push happen - with the repo left on whatever it was on.
    #[test]
    fn a_branch_is_reviewed_in_the_worktree_it_is_checked_out_in() {
        let fixture = crate::test_fixture::Fixture::new("review-request-worktree");
        let repo = fixture.root.clone();
        let beside = repo.join("../work-on-the-parser");
        crate::git::run_git_no_output(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "write-the-parser",
                &beside.display().to_string(),
            ],
        )
        .expect("failed to add the fixture worktree");
        let beside = beside.canonicalize().expect("the worktree is on disk");

        let on_the_branch = working_copy_of(&repo, Some("write-the-parser"));
        assert_eq!(
            on_the_branch, beside,
            "a branch checked out in a worktree is reviewed there"
        );
        assert_eq!(
            crate::git::current_branch_name(&repo)
                .expect("failed to read the branch")
                .as_deref(),
            Some("main"),
            "and the repo is left on the branch it was on"
        );

        // The branch the repo is itself on is the repo, not whatever the listing calls it - which
        // for a submodule is a path under `.git/modules` with no files in it.
        assert_eq!(working_copy_of(&repo, Some("main")), repo);
        // A branch checked out nowhere, and a line naming no branch, are both the repo.
        assert_eq!(working_copy_of(&repo, Some("never-made")), repo);
        assert_eq!(working_copy_of(&repo, None), repo);
    }

    /// A row stops being pending when the place it reviews has nothing left to commit - and that
    /// has to hold for a worktree too.
    ///
    /// This is why the count is taken here rather than off the submodule hub: the hub knows the
    /// repo and its submodules, and a worktree beside one is none of those. A row it could not
    /// be told about would read as pending however long ago the work was committed and pushed,
    /// leaving nothing to do but dismiss it by hand.
    #[test]
    fn a_committed_worktree_stops_reading_as_pending() {
        let fixture = crate::test_fixture::Fixture::new("review-request-pending");
        let repo = fixture.root.clone();
        let dir = repo.join(".moontasks/deploy-the-thing-1111");
        std::fs::create_dir_all(&dir).expect("failed to make the fixture task");
        std::fs::write(dir.join("metadata.json"), "{}\n").expect("failed to write the task");

        let beside = repo.join("../work-on-the-parser");
        crate::git::run_git_no_output(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "write-the-parser",
                &beside.display().to_string(),
            ],
        )
        .expect("failed to add the fixture worktree");
        let requested = repo.join(".moontasks/reviews/requested");
        std::fs::create_dir_all(&requested).expect("failed to make the fixture folder");
        std::fs::write(
            requested.join("parser.md"),
            "---\ntask: deploy-the-thing-1111\nrepo: .\nbranch: write-the-parser\n---\nfeat: it\n",
        )
        .expect("failed to write the fixture review");

        // Work in the worktree, which is what the line is asking to have looked at.
        std::fs::write(beside.join("parser.rs"), "pub fn parse() {}\n").expect("failed to write");
        let pending = |repo: &std::path::Path| list_for_repo(repo)[0].changed_files > 0;
        assert!(pending(&repo), "work waiting in the worktree is pending");

        crate::git::run_git_no_output(&beside, &["add", "-A"]).expect("failed to stage");
        assert!(pending(&repo), "staged and not committed is still pending");

        crate::git::run_git_no_output(&beside, &["commit", "-m", "feat: it"])
            .expect("failed to commit");
        assert!(
            !pending(&repo),
            "committed in the worktree, so there is nothing left to review there"
        );
    }

    #[test]
    fn a_repo_with_no_board_asks_for_nothing() {
        let fixture = crate::test_fixture::Fixture::new("review-request-none");

        assert!(list_for_repo(&fixture.root).is_empty());
    }
}
