//! One folder of a review's repo, listed for the tree beside a file tab - see
//! `crate::native::file_tree`.
//!
//! Unlike [`super::list_folder`], which lists any folder of the disk for the picker, this one
//! only reads under the session's repo, by a path relative to its root, and says which entries
//! git ignores: the tree hides those unless asked to show them.

use std::path::{Component, Path};

use anyhow::{Result, anyhow, bail};

use crate::api::{
    AppState,
    folders::{EntryKind, RepoEntry, RepoFolderListing},
};

/// How many names one `git check-ignore` is asked about. A `node_modules` holds thousands,
/// and one command line holding all of them can run past what the system takes.
const NAMES_PER_CHECK: usize = 400;

/// The folder `folder` of the session's repo - `""` for its root.
pub(crate) fn repo_folder(
    state: &AppState,
    session_id: &str,
    folder: &str,
) -> Result<RepoFolderListing> {
    let repo_path =
        crate::api::with_session(state, session_id, |session| Ok(session.repo_path.clone()))?;
    read_repo_folder(&repo_path, folder)
}

pub(crate) fn read_repo_folder(repo_path: &Path, folder: &str) -> Result<RepoFolderListing> {
    let folder = folder.trim_matches('/');
    // Only a path down from the root: the route is no way to list the rest of the disk.
    let goes_elsewhere = Path::new(folder)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)));
    if goes_elsewhere {
        bail!("{folder} is not a folder of the repo");
    }
    let path = repo_path.join(folder);
    let read = std::fs::read_dir(&path)
        .map_err(|error| anyhow!("{} cannot be listed: {error}", path.display()))?;

    let mut entries = Vec::new();
    for entry in read.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // The repo's own bookkeeping, never a file anyone opens from a tree.
        if folder.is_empty() && name == ".git" {
            continue;
        }
        let is_folder = std::fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir());
        entries.push(RepoEntry {
            name,
            kind: if is_folder {
                EntryKind::Folder
            } else {
                EntryKind::File
            },
            ignored: false,
        });
    }
    mark_ignored(repo_path, folder, &mut entries);
    entries.sort_by(|left, right| {
        (left.kind != EntryKind::Folder)
            .cmp(&(right.kind != EntryKind::Folder))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(RepoFolderListing {
        folder: folder.to_string(),
        entries,
    })
}

/// Set `ignored` on the entries git ignores. A folder that is not in a git repo ignores
/// nothing, and a git that cannot be asked leaves every entry shown: hiding files for an
/// error is worse than showing a few too many.
fn mark_ignored(repo_path: &Path, folder: &str, entries: &mut [RepoEntry]) {
    let paths: Vec<String> = entries
        .iter()
        .map(|entry| match folder {
            "" => entry.name.clone(),
            _ => format!("{folder}/{}", entry.name),
        })
        .collect();
    let mut ignored = std::collections::HashSet::new();
    for chunk in paths.chunks(NAMES_PER_CHECK) {
        let mut args = vec!["-c", "core.quotePath=false", "check-ignore", "--"];
        args.extend(chunk.iter().map(String::as_str));
        // 0: some are ignored, 1: none are.
        let Ok(said) = crate::git::run_git_allow_status(repo_path, &args, &[0, 1]) else {
            return;
        };
        ignored.extend(said.lines().map(str::to_string));
    }
    for (entry, path) in entries.iter_mut().zip(&paths) {
        entry.ignored = ignored.contains(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("moon-repo-folders-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let root = dir.as_path();
        crate::git::run_git(root, &["init", "-q"]).expect("git init");
        std::fs::write(root.join(".gitignore"), "/target\n*.log\n").expect("write");
        std::fs::create_dir_all(root.join("src/native")).expect("mkdir");
        std::fs::create_dir_all(root.join("target/debug")).expect("mkdir");
        std::fs::write(root.join("src/lib.rs"), "").expect("write");
        std::fs::write(root.join("src/run.log"), "").expect("write");
        std::fs::write(root.join("Zeta.md"), "").expect("write");
        std::fs::write(root.join("alpha.rs"), "").expect("write");
        dir
    }

    #[test]
    fn the_root_lists_folders_first_and_marks_what_git_ignores() {
        let dir = repo("root");
        let listing = read_repo_folder(&dir, "").expect("listing");
        let names: Vec<(&str, bool)> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.ignored))
            .collect();
        assert_eq!(
            names,
            vec![
                ("src", false),
                ("target", true),
                (".gitignore", false),
                ("alpha.rs", false),
                ("Zeta.md", false),
            ]
        );
    }

    #[test]
    fn a_subfolder_is_read_by_its_path_from_the_root() {
        let dir = repo("sub");
        let listing = read_repo_folder(&dir, "src/").expect("listing");
        assert_eq!(listing.folder, "src");
        let names: Vec<(&str, bool)> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.ignored))
            .collect();
        assert_eq!(
            names,
            vec![("native", false), ("lib.rs", false), ("run.log", true)]
        );
    }

    #[test]
    fn a_path_out_of_the_repo_is_refused() {
        let dir = repo("out");
        assert!(read_repo_folder(&dir, "../").is_err());
        assert!(read_repo_folder(&dir, "src/../..").is_err());
    }
}
