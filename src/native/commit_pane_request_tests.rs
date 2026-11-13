//! The commit a board task wrote for a repo, put in the commit pane's box - drawn and driven the
//! way the window draws it.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable};

use crate::{
    git::run_git_no_output,
    native::{
        commit_pane_tests::GIT_DEADLINE,
        panes::PaneKind,
        theme::ThemeMode,
        ui_tests::{Fixture, app_for, seeded_fixture},
    },
};

/// What a test watching the commit pane fill itself in reads, a frame at a time.
///
/// The window is drawn in a closure the harness owns, so what a test wants to assert on has to
/// be copied out of the app while it is in there. Every test below wants the same three things,
/// and has to wait on them in the same order - the pane opens, the board
/// poll answers, and only then is an empty box worth anything.
struct Watched {
    /// The panes that are open, which is how the test knows the commit pane arrived.
    panes: Arc<Mutex<Vec<PaneKind>>>,
    /// What is in the commit box, which is what these tests are about.
    message: Arc<Mutex<Option<String>>>,
    /// How many reviews the board read out of the review files. Zero until the poll answers.
    requests: Arc<Mutex<usize>>,
    /// What the pane found staged, once it has read the repo.
    staged: Arc<Mutex<Option<usize>>>,
}

impl Watched {
    fn panes(&self) -> Vec<PaneKind> {
        self.panes.lock().expect("expected the lock").clone()
    }

    fn message(&self) -> Option<String> {
        self.message.lock().expect("expected the lock").clone()
    }

    fn requests(&self) -> usize {
        *self.requests.lock().expect("expected the lock")
    }

    fn staged(&self) -> Option<usize> {
        *self.staged.lock().expect("expected the lock")
    }
}

/// Write a task on the fixture's board, and a review file of it in `status` asking for that
/// commit on that branch of the board's own repo.
fn a_task_asking_for_review(
    fixture: &Fixture,
    task: &str,
    status: &str,
    branch: &str,
    message: &str,
) {
    fixture.write(
        &format!(".moontasks/{task}/metadata.json"),
        "{\n  \"title\": \"Deploy the thing\",\n  \"status\": \"in_progress\",\n  \
         \"created_at_unix\": 1700000000,\n  \"resources\": []\n}\n",
    );
    fixture.write_review_file(
        status,
        task,
        &match branch {
            "" => format!("task: {task}\nrepo: .\n"),
            branch => format!("task: {task}\nrepo: .\nbranch: {branch}\n"),
        },
        message,
    );
}

/// Where a review waits to be looked at, which is where these reviews are unless a test is about
/// one that is done.
const REQUESTED: &str = crate::moontasks::reviews::FIRST_REVIEW_STATUS;

/// Open the fixture's review, press its commit button, and wait for the pane and for the board's
/// reading of the tasks' files - which is everything the message in the box depends on.
fn commit_pane_over_the_board(fixture: &Fixture) -> (Harness<'static>, Watched) {
    let mut app = app_for(&fixture.root, ThemeMode::Dark);
    let watched = Watched {
        panes: Arc::new(Mutex::new(Vec::new())),
        message: Arc::new(Mutex::new(None)),
        requests: Arc::new(Mutex::new(0)),
        staged: Arc::new(Mutex::new(None)),
    };
    let panes = Arc::clone(&watched.panes);
    let message = Arc::clone(&watched.message);
    let requests = Arc::clone(&watched.requests);
    let staged = Arc::clone(&watched.staged);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 940.0))
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |ui| {
            let session_id = app.model.root_session_id.clone();
            app.draw(ui);
            *requests.lock().expect("expected the lock") = app.model.review_requests.len();
            *staged.lock().expect("expected the lock") = app
                .model
                .commit_panes
                .get(&session_id)
                .and_then(|pane| pane.staged_count_for_test());
            *panes.lock().expect("expected the lock") = app
                .model
                .layout
                .panes()
                .map(|(_, pane)| pane.kind())
                .collect();
            *message.lock().expect("expected the lock") = app
                .model
                .commit_panes
                .get(&session_id)
                .map(|pane| pane.message.clone());
        });

    let deadline = Instant::now() + GIT_DEADLINE;
    while Instant::now() < deadline && !watched.panes().contains(&PaneKind::Commit) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
        if harness.query_by_label("[commit]").is_some() {
            harness.get_by_label("[commit]").click();
        }
    }

    // The poll that reads the board's reviews is a worker thread, so a review lands a few frames
    // after the pane does.
    let deadline = Instant::now() + GIT_DEADLINE;
    while Instant::now() < deadline && watched.requests() == 0 {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        watched.requests() > 0,
        "the board should have read the review file"
    );

    (harness, watched)
}

/// Wait for the pane to have read the repo, which is what tells it which branch it is on - and
/// so the last thing the message in the box waits on. An empty box before this proves nothing.
fn wait_for_the_repo_to_be_read(harness: &mut Harness<'static>, watched: &Watched) {
    let deadline = Instant::now() + GIT_DEADLINE;
    while Instant::now() < deadline && watched.staged().is_none() {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        watched.staged().is_some(),
        "the pane should have read the repo"
    );
}

/// The branch the fixture's task asks its commit to be made on, what its review file says to
/// commit there, and what the box should end up holding for it.
const REQUESTED_BRANCH: &str = "ship-it";
const REQUESTED_BODY: &str =
    "feat: add the extra module\n\nThe fixture gains a module the review is of.\n";
const REQUESTED_MESSAGE: &str =
    "feat: add the extra module\n\nThe fixture gains a module the review is of.";

/// A commit an agent already wrote is the one the pane opens with.
///
/// When a review file names this repo and the branch it is on and says what to commit there, that message is in the box the moment the pane opens - no `opencode`, no run,
/// nothing to press, and nothing staged yet either. It does not come from the diff, so it does
/// not wait on one: what is about to be committed is readable while the hunks are still being
/// picked next door.
#[test]
fn a_commit_a_task_asked_for_is_in_the_box_before_anything_is_staged() {
    let fixture = seeded_fixture("commit-message-requested");
    fixture.checkout_branch("ship-it");
    a_task_asking_for_review(
        &fixture,
        "deploy-the-thing-1111",
        REQUESTED,
        REQUESTED_BRANCH,
        REQUESTED_BODY,
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);

    let deadline = Instant::now() + GIT_DEADLINE;
    while Instant::now() < deadline && watched.message().as_deref() != Some(REQUESTED_MESSAGE) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(
        watched.message().as_deref(),
        Some(REQUESTED_MESSAGE),
        "the commit the task asked for should be in the box with nothing staged"
    );
    assert!(
        harness.query_by_label("use").is_none(),
        "the message is in the box, so there is nothing to press to put it there"
    );
    // The pane's reading of the repo lands on its own clock, after the message did - so it is
    // waited for before it is asked what was staged.
    wait_for_the_repo_to_be_read(&mut harness, &watched);
    assert_eq!(
        watched.staged(),
        Some(0),
        "and it got there without a thing being staged"
    );

    // The header says the repo is on the branch the review asked for, so there is no pill saying
    // the commit belongs somewhere else.
}

/// A review about a branch this repo is not on keeps its message to itself.
///
/// A review names the branch its commit belongs on. When that branch is checked out nowhere - the
/// usual reason being that it was merged and everyone moved off it - the review resolves back onto
/// the main checkout, and the next piece of work written there would open its commit pane on
/// someone else's finished message. The box stays empty, and the header says whose message it
/// was and why it is not there.
#[test]
fn a_commit_asked_for_on_another_branch_stays_out_of_the_box() {
    let fixture = seeded_fixture("commit-message-other-branch");
    fixture.checkout_branch("the-next-thing");
    a_task_asking_for_review(
        &fixture,
        "deploy-the-thing-1111",
        REQUESTED,
        REQUESTED_BRANCH,
        REQUESTED_BODY,
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);
    wait_for_the_repo_to_be_read(&mut harness, &watched);

    assert_eq!(
        watched.message().as_deref(),
        Some(""),
        "a commit written for another branch should not be put in this one's box"
    );

    // The header carries the `asked for ship-it` pill, which is what says whose message this was
    // and why it is not in the box.
}

/// A review whose commit has been made keeps its message to itself.
///
/// The review is committed and left in `requested/` while the agent goes on working in the
/// repo, and what it stages next is another commit. Putting the review's message in the box again
/// - which a freshly opened window would, having never put it there - would be offering the
/// commit already made for work it does not describe.
#[test]
fn a_commit_already_made_stays_out_of_the_box() {
    let fixture = seeded_fixture("commit-message-already-made");
    fixture.checkout_branch("ship-it");
    fixture.write("extra.txt", "the module\n");
    fixture.commit("feat: add the extra module\n\nThe fixture gains a module the review is of.");
    fixture.write("later.txt", "the work after it\n");
    a_task_asking_for_review(
        &fixture,
        "deploy-the-thing-1111",
        REQUESTED,
        REQUESTED_BRANCH,
        REQUESTED_BODY,
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);
    wait_for_the_repo_to_be_read(&mut harness, &watched);

    assert_eq!(
        watched.message().as_deref(),
        Some(""),
        "a commit already made should not be offered for the work after it"
    );
}

/// A review of a commit already made stays out of the way of the one waiting in the tree.
///
/// A review naming `commit:` is about that commit - often an old one, in `done/`, written after
/// the review of the work in the tree. Taken as the newest review of the repo, it would leave the
/// box empty for the work that is actually waiting.
#[test]
fn a_review_of_a_past_commit_leaves_the_box_to_the_work_in_the_tree() {
    let fixture = seeded_fixture("commit-message-past-commit-review");
    // Neither review names a branch, so the newest of the repo's is the one the box goes by.
    a_task_asking_for_review(&fixture, "deploy-the-thing-1111", REQUESTED, "", REQUESTED_BODY);
    // Written a second later, so it is the newest review of the repo - files are told apart by
    // the second they were written in.
    std::thread::sleep(Duration::from_millis(1100));
    let past = crate::git::run_git(&fixture.root, &["rev-parse", "HEAD"])
        .expect("the fixture has a commit");
    fixture.write_review_file(
        crate::moontasks::reviews::DONE_REVIEW_STATUS,
        "an-older-change",
        &format!("task: deploy-the-thing-1111\nrepo: .\ncommit: {}\n", past.trim()),
        "fix: an older change\n",
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);

    let deadline = Instant::now() + GIT_DEADLINE;
    while Instant::now() < deadline && watched.message().as_deref() != Some(REQUESTED_MESSAGE) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        watched.message().as_deref(),
        Some(REQUESTED_MESSAGE),
        "the working tree's review should fill the box, not the review of a past commit"
    );
}

/// The commit that lands in the box is the one written for the branch the repo is on.
///
/// Several tasks name the same repo, and a review naming a branch that is checked out nowhere
/// resolves to that repo all the same - so every one of them points at this one working copy.
/// The branch is what tells them apart: the pane is committing the work of the task whose branch
/// is out, and it is that task's message that belongs in the box, wherever its review sits among
/// the others - here after another task's, which is the one the repo alone would have found.
#[test]
fn the_commit_in_the_box_is_the_one_written_for_the_branch_that_is_out() {
    let fixture = seeded_fixture("commit-message-two-reviews");
    fixture.checkout_branch("ship-it");
    a_task_asking_for_review(
        &fixture,
        "aaa-earlier-task-1111",
        REQUESTED,
        "some-older-branch",
        "feat: something else entirely\n\nFinished a fortnight ago.\n",
    );
    a_task_asking_for_review(
        &fixture,
        "zzz-later-task-2222",
        REQUESTED,
        REQUESTED_BRANCH,
        REQUESTED_BODY,
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);

    let deadline = Instant::now() + GIT_DEADLINE;
    while Instant::now() < deadline && watched.message().as_deref() != Some(REQUESTED_MESSAGE) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        watched.message().as_deref(),
        Some(REQUESTED_MESSAGE),
        "the review for the branch the repo is on should be the one in the box"
    );
}

/// A review in `done/` is finished with, and its message is not offered again.
///
/// Moving a review to `done/` is how work that is committed and pushed stops asking to be looked
/// at. The repo going dirty again is the next piece of work, not that one coming back.
#[test]
fn a_review_that_is_done_keeps_its_message_out_of_the_box() {
    let fixture = seeded_fixture("commit-message-review-done");
    fixture.checkout_branch("ship-it");
    a_task_asking_for_review(
        &fixture,
        "deploy-the-thing-1111",
        crate::moontasks::reviews::DONE_REVIEW_STATUS,
        REQUESTED_BRANCH,
        REQUESTED_BODY,
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);
    wait_for_the_repo_to_be_read(&mut harness, &watched);

    assert_eq!(
        watched.message().as_deref(),
        Some(""),
        "a review that is done should not put its message in the box"
    );
}

/// A repo gets one commit after another through the same pane, and each review asking for one
/// fills the box in its turn.
///
/// The pane is kept for as long as the window is open - the session it belongs to is the repo -
/// so the first commit it ever filled in is not the last. Once that one is made, its review is
/// still in `requested/` and still names a branch, and it does not come back into the emptied box.
/// The next piece of work in the repo, on its own branch with its own review, is a different
/// commit, and its message goes in the box the way the first one's did.
#[test]
fn the_next_commit_a_task_asked_for_fills_the_box_once_the_last_one_is_made() {
    let fixture = seeded_fixture("commit-message-next-request");
    fixture.checkout_branch("ship-it");
    run_git_no_output(&fixture.root, &["add", "-A"]).expect("failed to stage the fixture");
    a_task_asking_for_review(
        &fixture,
        "deploy-the-thing-1111",
        REQUESTED,
        REQUESTED_BRANCH,
        REQUESTED_BODY,
    );

    let (mut harness, watched) = commit_pane_over_the_board(&fixture);
    let step_until = |harness: &mut Harness<'static>, done: &dyn Fn() -> bool| {
        let deadline = Instant::now() + GIT_DEADLINE;
        while Instant::now() < deadline && !done() {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
    };

    // The commit button is off until the pane has seen what is staged.
    step_until(&mut harness, &|| {
        watched.message().as_deref() == Some(REQUESTED_MESSAGE)
            && watched.staged().is_some_and(|count| count > 0)
    });
    assert_eq!(watched.message().as_deref(), Some(REQUESTED_MESSAGE));

    harness.get_by_label("commit").click();
    let committed = || {
        crate::git::run_git(&fixture.root, &["log", "-1", "--format=%s"])
            .is_ok_and(|subject| subject.trim() == "feat: add the extra module")
    };
    step_until(&mut harness, &|| {
        committed() && watched.message().as_deref() == Some("")
    });
    assert!(committed(), "the requested commit should have been made");
    assert_eq!(
        watched.message().as_deref(),
        Some(""),
        "and the box emptied"
    );

    // The review just committed is still there, on the branch still out: it is not offered again.
    wait_for_the_repo_to_be_read(&mut harness, &watched);
    harness.run_steps(10);
    assert_eq!(
        watched.message().as_deref(),
        Some(""),
        "the commit just made should not be put back in the box"
    );

    // The next piece of work: its own branch, its own task, its own review.
    fixture.checkout_branch("the-next-thing");
    fixture.write("src/next.rs", "pub fn next() {}\n");
    a_task_asking_for_review(
        &fixture,
        "the-next-thing-2222",
        REQUESTED,
        "the-next-thing",
        "feat: the next thing\n\nIts own review, on its own branch.\n",
    );

    step_until(&mut harness, &|| {
        watched.message().as_deref()
            == Some("feat: the next thing\n\nIts own review, on its own branch.")
    });
    assert_eq!(
        watched.message().as_deref(),
        Some("feat: the next thing\n\nIts own review, on its own branch."),
        "the next commit a task asked for should be in the box"
    );
}
