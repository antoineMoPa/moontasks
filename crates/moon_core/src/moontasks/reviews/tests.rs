use super::*;

#[test]
fn a_review_file_reads_its_keys_and_its_commit_message() {
    let review = parse_review_file(
        "encabulator-fix-the-races",
        "reviewing",
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
        ["requested", "reviewing", "done", "qa"]
    );
    move_review_file(&board.root, "a", Some("reviewing")).expect("expected the move");
    let reviews = read_review_files(&board.root);
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].status, "reviewing");

    move_review_file(&board.root, "a", None).expect("expected the removal");
    assert!(read_review_files(&board.root).is_empty());
    assert!(move_review_file(&board.root, "../x", Some("done")).is_err());
}
