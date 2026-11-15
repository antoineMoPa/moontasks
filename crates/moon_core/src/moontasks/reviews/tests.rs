use super::*;

#[test]
fn a_review_file_reads_its_keys_and_its_commit_message() {
    let review = parse_review_file(
        "encabulator-fix-the-races",
        "committed",
        "---\ntask: fix-the-races-3c62\nrepo: repos/retro_encabulator/\nbranch: fix-the-races\n\
         pr: 412\ndeploy: pending\norder: 2\n---\nfix(encabulator): put the bearing races back\n\n\
         They were reversed.\n",
    )
    .expect("expected the file to read");
    assert_eq!(review.task.as_deref(), Some("fix-the-races-3c62"));
    assert_eq!(review.path_under_repo, "repos/retro_encabulator");
    assert_eq!(review.branch.as_deref(), Some("fix-the-races"));
    assert_eq!(review.pr.as_deref(), Some("412"));
    assert_eq!(review.deploy.as_deref(), Some("pending"));
    assert_eq!(review.order, Some(2));
    let suggestion = review.suggestion.expect("expected a commit message");
    assert_eq!(suggestion.subject, "fix(encabulator): put the bearing races back");
    assert_eq!(suggestion.paragraph, "They were reversed.");
}

#[test]
fn a_review_file_with_only_a_repo_is_the_boards_own_repo_when_it_says_dot() {
    let review = parse_review_file("root", "requested", "---\nrepo: .\n---\n")
        .expect("expected the file to read");
    assert_eq!(review.path_under_repo, "");
    assert_eq!(review.suggestion, None);
}

#[test]
fn a_key_the_board_does_not_know_is_refused() {
    let error = parse_review_file("x", "requested", "---\nrepo: .\nstatus: done\n---\n")
        .expect_err("expected status: to be refused");
    assert!(error.to_string().contains("status"));
}

#[test]
fn the_pull_request_is_described_by_its_number_or_its_branch() {
    assert_eq!(describe_pull_request(Some("412"), Some("b")), "PR 412");
    assert_eq!(
        describe_pull_request(Some("https://github.com/o/r/pull/9"), None),
        "PR 9"
    );
    assert_eq!(describe_pull_request(None, Some("main")), "#main");
}

#[test]
fn moving_a_review_moves_its_file_between_status_folders() {
    let board = crate::test_fixture::Fixture::new("review-files-move");
    let root = reviews_root(&board.root);
    std::fs::create_dir_all(root.join("requested")).expect("expected the folder");
    std::fs::create_dir_all(root.join("qa")).expect("expected the folder");
    std::fs::write(
        root.join("requested/a.md"),
        "---\nrepo: repos/a\norder: 1\n---\nfeat: a\n",
    )
    .expect("expected the file");

    assert_eq!(
        list_review_statuses(&board.root),
        ["requested", "committed", "pushed", "deployed", "qa"]
    );
    move_review_file(&board.root, "a", Some("deployed")).expect("expected the move");
    let reviews = read_review_files(&board.root);
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].status, "deployed");

    move_review_file(&board.root, "a", None).expect("expected the removal");
    assert!(read_review_files(&board.root).is_empty());
    assert!(move_review_file(&board.root, "../x", Some("committed")).is_err());
}

/// A review moves to `committed/` once a commit with its subject is made - with that commit
/// written into its file - and to `pushed/` once a remote branch has it.
#[test]
fn a_review_follows_its_commit_to_committed_and_then_pushed() {
    let board = crate::test_fixture::Fixture::new("review-files-git-stage");
    let git = |args: &[&str]| crate::git::run_git_no_output(&board.root, args).expect("git");
    git(&["commit", "--allow-empty", "-m", "chore: start"]);
    let root = reviews_root(&board.root);
    std::fs::create_dir_all(root.join("requested")).expect("expected the folder");
    std::fs::write(
        root.join("requested/parser.md"),
        "---\nrepo: .\n---\nfeat(parser): read the files\n",
    )
    .expect("expected the file");

    move_reviews_to_their_git_stage(&board.root);
    assert_eq!(read_review_files(&board.root)[0].status, "requested");

    git(&[
        "commit",
        "--allow-empty",
        "-m",
        "feat(parser): read the files",
    ]);
    move_reviews_to_their_git_stage(&board.root);
    let review = &read_review_files(&board.root)[0];
    assert_eq!(review.status, "committed");
    let head = crate::git::run_git(&board.root, &["rev-parse", "HEAD"]).expect("head");
    assert_eq!(review.commit.as_deref(), Some(head.trim()));
    assert_eq!(
        review.suggestion.as_ref().map(|s| s.subject.as_str()),
        Some("feat(parser): read the files")
    );

    // A pushed commit is one a remote-tracking branch has.
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    move_reviews_to_their_git_stage(&board.root);
    assert_eq!(read_review_files(&board.root)[0].status, "pushed");

    // The remote losing it puts it back to committed; deployed is never touched.
    git(&["update-ref", "-d", "refs/remotes/origin/main"]);
    move_reviews_to_their_git_stage(&board.root);
    assert_eq!(read_review_files(&board.root)[0].status, "committed");
    move_review_file(&board.root, "parser", Some("deployed")).expect("expected the move");
    move_reviews_to_their_git_stage(&board.root);
    assert_eq!(read_review_files(&board.root)[0].status, "deployed");
}

/// The folders older boards had are emptied into the ones that replaced them.
#[test]
fn an_old_boards_review_folders_move_to_the_new_statuses() {
    let board = crate::test_fixture::Fixture::new("review-files-old-folders");
    let root = reviews_root(&board.root);
    for (folder, id) in [("reviewing", "a"), ("done", "b")] {
        std::fs::create_dir_all(root.join(folder)).expect("expected the folder");
        std::fs::write(
            root.join(folder).join(format!("{id}.md")),
            "---\nrepo: .\n---\n",
        )
        .expect("expected the file");
    }

    move_reviews_to_their_git_stage(&board.root);

    let statuses: Vec<(String, String)> = read_review_files(&board.root)
        .into_iter()
        .map(|review| (review.id, review.status))
        .collect();
    assert_eq!(
        statuses,
        [
            ("a".into(), "requested".into()),
            ("b".into(), "committed".into())
        ]
    );
    assert!(!root.join("reviewing").exists() && !root.join("done").exists());
}
