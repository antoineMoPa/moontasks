//! The files outside the repo a language server has named, which are the only ones outside it
//! that may be read - see [`FilesNamedOutsideTheRepo`]. Kept beside git's file reads, which
//! ask it, rather than with the language servers, which only fill it in.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

/// How many files outside the repo one session remembers having been told about.
///
/// Every ⌘-click into a dependency adds one, so the list grows with the reading rather than
/// with anything an attacker controls; a session that spends an afternoon in `~/.cargo` still
/// has a bounded list, and the oldest jump falling off it only means that tab has to be
/// reopened by clicking through to it again.
const FILES_REMEMBERED: usize = 512;

/// The files outside the repo that a language server has named in this session, and the only
/// ones outside it that may be read.
///
/// A definition in a Rust project lands in `~/.cargo/registry` or in the standard library as
/// often as it lands in the repo, so the pane has to be able to open what the jump landed on.
/// But the repo side of a `--remote` session is a server on somebody else's machine, and a
/// read that took whatever absolute path it was handed would be arbitrary file read on that
/// machine - `~/.ssh/id_rsa` for the asking. So the only paths outside the repo that can be
/// read are the ones a server itself named while answering a question the person asked, and
/// each one is remembered here on its way back to the window.
///
/// Resolved paths, never the strings that were handed in: `..` segments and symlinks are how
/// a list like this gets walked around, so both what is remembered and what is later asked
/// for go through [`real_path`] before they are ever compared.
#[derive(Default)]
pub struct FilesNamedOutsideTheRepo {
    /// Oldest first, which is what a full list drops.
    remembered: VecDeque<PathBuf>,
}

impl FilesNamedOutsideTheRepo {
    /// Remember a file a server named, if it named one outside the repo at all. A path
    /// inside the repo is relative and is read the way every other file of the repo is, so
    /// there is nothing to remember about it.
    pub fn remember(&mut self, file_path: &str) {
        let Some(real_path) = real_path(file_path) else {
            return;
        };
        if self.remembered.contains(&real_path) {
            return;
        }
        if self.remembered.len() == FILES_REMEMBERED {
            self.remembered.pop_front();
        }
        self.remembered.push_back(real_path);
    }

    /// The file on disk this path names, when it is one a server named. `None` for everything
    /// else, which is every path that has to go on being refused - see
    /// [`crate::git::read_repo_file`].
    pub fn allows(&self, file_path: &str) -> Option<PathBuf> {
        let real_path = real_path(file_path)?;
        self.remembered.contains(&real_path).then_some(real_path)
    }
}

/// The file a path really names, with every `..` segment and every symlink already followed.
///
/// Only an absolute path is resolved. A relative one is a path in the repo - which is the
/// other read route entirely - and resolving it here would resolve it against whatever
/// directory this process happens to be running in, which is not something either side of
/// the comparison should depend on.
fn real_path(file_path: &str) -> Option<PathBuf> {
    let path = Path::new(file_path);
    if !path.is_absolute() {
        return None;
    }
    path.canonicalize().ok()
}
