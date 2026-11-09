//! Uninstalling - takes off this machine what `install.sh` put on it: the `moon` executable
//! in the install folder, and the launchers the OS lists.
//!
//! It is the install script read backwards, and no more than that. What a person made with
//! the program stays: their settings, their boards, the `.moontasks` folder of each repo. So
//! does the `PATH` line the script may have added to a shell's file - `~/.local/bin` is a
//! folder other programs are installed in too.
//!
//! A `moon` installed some other way - `cargo install`, a build in `target/` - is not the
//! script's to take away, and is named in [`Uninstalled::left_in_place`] rather than deleted:
//! `cargo uninstall` is what keeps cargo's own record of it straight.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};

use crate::{
    cli::PROGRAM,
    native::{launchers, programs::this_executable},
};

/// A command only this program's help lists, which is how an executable called `moon` is
/// told from another program of the same name - there is one, MoonBit's build tool.
const COMMAND_ONLY_WE_HAVE: &str = "install-launchers";

/// What an uninstall took away, and what it found and left.
pub(crate) struct Uninstalled {
    /// The executable and the launchers that were deleted, in the order they went.
    pub(crate) removed: Vec<PathBuf>,
    /// The executable this process runs, when it is not the one the install script wrote and
    /// so was not deleted.
    pub(crate) left_in_place: Option<PathBuf>,
}

/// Where `install.sh` puts the executable: the folder its `MOONTASKS_INSTALL_DIR` names, and
/// `~/.local/bin` when nothing names one.
pub(crate) fn find_install_script_executable_path() -> Result<PathBuf> {
    let install_dir = ["MOONTASKS_INSTALL_DIR", "MOONREVIEW_INSTALL_DIR"]
        .iter()
        .filter_map(env::var_os)
        .find(|dir| !dir.is_empty())
        .map(PathBuf::from);
    let install_dir = match install_dir {
        Some(dir) => dir,
        None => env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set, so there is no install folder to look in")?
            .join(".local/bin"),
    };
    Ok(install_dir.join(PROGRAM))
}

/// Delete the launchers and the executable the install script wrote.
///
/// The launchers go first: each is a link to the executable, and one left behind by a
/// failure halfway would open nothing.
pub(crate) fn uninstall_moon() -> Result<Uninstalled> {
    let running = this_executable().ok();
    let mut removed = launchers::remove_installed_launchers()?;

    let installed = find_install_script_executable_path()?;
    let removed_executable = remove_executable_if_ours(&installed, running.as_deref())?;
    if removed_executable {
        removed.push(installed.clone());
    }

    let left_in_place = running.filter(|running| !is_same_file(running, &installed));
    Ok(Uninstalled {
        removed,
        left_in_place,
    })
}

/// Delete the executable at `installed` when it is this program, and say whether it went.
/// `running` is the executable this process runs, which is this program without asking.
fn remove_executable_if_ours(installed: &Path, running: Option<&Path>) -> Result<bool> {
    if installed.symlink_metadata().is_err() {
        return Ok(false);
    }
    let is_ours = running.is_some_and(|running| is_same_file(running, installed))
        || lists_command_only_we_have(installed);
    if !is_ours {
        return Ok(false);
    }

    // Deleting the file a process runs is fine on Unix: the process keeps what it has open,
    // so the window that asked for this stays up until it is closed.
    fs::remove_file(installed)
        .with_context(|| format!("failed to remove {}", installed.display()))?;
    Ok(true)
}

/// Whether two paths are one file, through whatever links lead to it.
fn is_same_file(one: &Path, other: &Path) -> bool {
    match (one.canonicalize(), other.canonicalize()) {
        (Ok(one), Ok(other)) => one == other,
        _ => false,
    }
}

/// Whether the help of the executable at `path` lists [`COMMAND_ONLY_WE_HAVE`].
fn lists_command_only_we_have(path: &Path) -> bool {
    Command::new(path)
        .arg("--help")
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(COMMAND_ONLY_WE_HAVE))
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A folder of this test's own, with a `moon` in it that answers `--help` with `help`.
    fn write_executable_answering_help(test: &str, help: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("moon-uninstalling-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("expected the folder to be made");
        let executable = dir.join(PROGRAM);
        fs::write(&executable, format!("#!/bin/sh\necho '{help}'\n"))
            .expect("expected the script to be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("expected the script to be made executable");
        executable
    }

    #[test]
    fn an_executable_whose_help_is_ours_is_removed() {
        let executable =
            write_executable_answering_help("ours", "  moon install-launchers  add launchers");

        let removed = remove_executable_if_ours(&executable, None).expect("expected no failure");

        assert!(removed);
        assert!(!executable.exists());
    }

    /// MoonBit's build tool is called `moon` too, and is not ours to delete.
    #[test]
    fn another_program_of_the_same_name_is_left() {
        let executable = write_executable_answering_help("foreign", "moon build  build a package");

        let removed = remove_executable_if_ours(&executable, None).expect("expected no failure");

        assert!(!removed);
        assert!(executable.exists());
    }

    /// The executable this process runs is ours without asking it anything.
    #[test]
    fn the_running_executable_is_removed_without_asking_its_help() {
        let executable = write_executable_answering_help("running", "says nothing of ours");

        let removed =
            remove_executable_if_ours(&executable, Some(&executable)).expect("expected no failure");

        assert!(removed);
        assert!(!executable.exists());
    }

    #[test]
    fn nothing_installed_is_nothing_removed() {
        let missing = env::temp_dir().join("moon-uninstalling-missing/moon");

        let removed = remove_executable_if_ours(&missing, None).expect("expected no failure");

        assert!(!removed);
    }
}
