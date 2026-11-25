//! `stage all`: staging every change the review shows, and nothing it does not.

use std::{path::Path, process::Stdio};

use anyhow::{Context, Result, bail};

use super::repo_side::read_status;
use crate::{agent::ChildExt, git};

/// Stage every path `git status` lists with work outside the index, untracked files included.
///
/// Not `git add -A`: that one also stages the new commit of a submodule the repo is set to
/// ignore (`submodule.<name>.ignore`), which neither the status nor the diff the review is
/// drawn from shows - so a commit would carry a submodule bump nobody looked at. The paths
/// staged here are the ones the pane counted on its `stage all (N)` button.
pub fn stage_every_unstaged_path(repo_path: &Path, pathspec: Option<&str>) -> Result<()> {
    let (_, unstaged_paths) = read_status(repo_path, pathspec)?;
    if unstaged_paths.is_empty() {
        return Ok(());
    }

    // Over stdin, NUL between them: any number of paths, each taken as the name it is rather
    // than as a pattern.
    let mut paths = unstaged_paths.join("\0").into_bytes();
    paths.push(0);
    let output = git::git_command(repo_path)?
        .args([
            "--literal-pathspecs",
            "add",
            "--pathspec-from-file=-",
            "--pathspec-file-nul",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start git add")?
        .wait_with_output_from_stdin(&paths, "failed to write the paths to git add")?;

    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn create_repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("moontasks-stage-all-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("failed to create the fixture");
        git::run_git_no_output(&root, &["init", "--initial-branch=main"]).expect("failed to init");
        for (key, value) in [
            ("user.email", "test@example.com"),
            ("user.name", "Test"),
            ("commit.gpgsign", "false"),
        ] {
            git::run_git_no_output(&root, &["config", key, value]).expect("failed to configure");
        }
        root
    }

    fn commit_file(repo: &Path, file: &str, content: &str) {
        std::fs::write(repo.join(file), content).expect("failed to write");
        git::run_git_no_output(repo, &["add", "--", file]).expect("failed to stage");
        git::run_git_no_output(repo, &["commit", "-m", file]).expect("failed to commit");
    }

    fn list_staged_paths(repo: &Path) -> Vec<String> {
        git::run_git(
            repo,
            &[
                "diff",
                "--cached",
                "--name-only",
                "--ignore-submodules=none",
            ],
        )
        .expect("failed to read the index")
        .lines()
        .map(str::to_string)
        .collect()
    }

    #[test]
    fn every_change_the_status_lists_is_staged() {
        let repo = create_repo("changes");
        commit_file(&repo, "changed.txt", "one\n");
        commit_file(&repo, "deleted.txt", "one\n");
        std::fs::write(repo.join("changed.txt"), "two\n").expect("failed to change");
        std::fs::remove_file(repo.join("deleted.txt")).expect("failed to delete");
        std::fs::create_dir_all(repo.join("new folder")).expect("failed to create a folder");
        std::fs::write(repo.join("new folder/a*.txt"), "new\n").expect("failed to write");

        stage_every_unstaged_path(&repo, None).expect("expected the changes staged");

        assert_eq!(
            list_staged_paths(&repo),
            ["changed.txt", "deleted.txt", "new folder/a*.txt"]
        );
        let (_, unstaged) = read_status(&repo, None).expect("expected a status");
        assert!(unstaged.is_empty(), "left unstaged: {unstaged:?}");

        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn only_what_the_review_is_pointed_at_is_staged() {
        let repo = create_repo("pathspec");
        commit_file(&repo, "root.txt", "one\n");
        std::fs::create_dir_all(repo.join("part")).expect("failed to create a folder");
        std::fs::write(repo.join("part/inside.txt"), "new\n").expect("failed to write");
        std::fs::write(repo.join("root.txt"), "two\n").expect("failed to change");

        stage_every_unstaged_path(&repo, Some("part")).expect("expected the part staged");

        assert_eq!(list_staged_paths(&repo), ["part/inside.txt"]);

        let _ = std::fs::remove_dir_all(&repo);
    }

    /// The bug this replaced `git add -A` for: a submodule set to `ignore = all` is in no
    /// status and no diff, yet `git add -A` stages the commit it moved to.
    #[test]
    fn a_submodule_the_repo_ignores_is_left_out() {
        let child = create_repo("ignored-child");
        commit_file(&child, "lib.rs", "// lib\n");
        let repo = create_repo("ignored-parent");
        git::run_git_no_output(
            &repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                child.to_str().expect("a utf-8 path"),
                "child",
            ],
        )
        .expect("failed to add the submodule");
        git::run_git_no_output(&repo, &["commit", "-m", "Add the submodule"])
            .expect("failed to commit");
        git::run_git_no_output(&repo, &["config", "submodule.child.ignore", "all"])
            .expect("failed to ignore the submodule");
        let submodule = repo.join("child");
        for (key, value) in [
            ("user.email", "test@example.com"),
            ("user.name", "Test"),
            ("commit.gpgsign", "false"),
        ] {
            git::run_git_no_output(&submodule, &["config", key, value])
                .expect("failed to configure");
        }
        commit_file(&submodule, "more.rs", "// more\n");
        std::fs::write(repo.join("seen.txt"), "new\n").expect("failed to write");

        stage_every_unstaged_path(&repo, None).expect("expected the changes staged");

        assert_eq!(list_staged_paths(&repo), ["seen.txt"]);

        // What this guards against is real: the plain sweep does take the submodule.
        git::run_git_no_output(&repo, &["add", "-A"]).expect("failed to stage");
        assert_eq!(list_staged_paths(&repo), ["child", "seen.txt"]);

        for root in [repo, child] {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
