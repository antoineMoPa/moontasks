//! The rows the tree shows, worked out from what has been listed: the folders that are open,
//! or the paths a filter found.

use std::collections::HashSet;

use crate::api::folders::EntryKind;

use super::RepoTree;

pub(super) enum RowKind {
    Folder {
        open: bool,
    },
    File,
    /// A line that is not a path - a folder still being listed, or why it could not be.
    Note,
}

pub(super) struct Row {
    pub(super) depth: usize,
    pub(super) name: String,
    /// From the repo's root, no `/` at either end.
    pub(super) path: String,
    pub(super) kind: RowKind,
    pub(super) ignored: bool,
}

/// The path of `name` in `folder`.
pub(super) fn join(folder: &str, name: &str) -> String {
    match folder {
        "" => name.to_string(),
        _ => format!("{folder}/{name}"),
    }
}

/// Every folder `file_path` is under, from the root down - `src`, `src/native`.
pub(super) fn ancestors(file_path: &str) -> Vec<String> {
    let parts: Vec<&str> = file_path.split('/').collect();
    (1..parts.len()).map(|end| parts[..end].join("/")).collect()
}

/// The rows of the open folders, walked from the root. Each folder walked whose listing is
/// missing or old goes into `to_ask`.
pub(super) fn tree_rows(
    tree: &RepoTree,
    show_ignored: bool,
    stale: &dyn Fn(&super::Folder) -> bool,
    to_ask: &mut Vec<String>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    walk(tree, "", 0, show_ignored, false, stale, to_ask, &mut rows);
    rows
}

#[allow(clippy::too_many_arguments)]
fn walk(
    tree: &RepoTree,
    folder: &str,
    depth: usize,
    show_ignored: bool,
    under_ignored: bool,
    stale: &dyn Fn(&super::Folder) -> bool,
    to_ask: &mut Vec<String>,
    rows: &mut Vec<Row>,
) {
    let note = |name: String| Row {
        depth,
        name,
        path: String::new(),
        kind: RowKind::Note,
        ignored: false,
    };
    let Some(listed) = tree.folders.get(folder) else {
        to_ask.push(folder.to_string());
        rows.push(note("loading…".to_string()));
        return;
    };
    if stale(listed) {
        to_ask.push(folder.to_string());
    }
    let Some(entries) = &listed.entries else {
        rows.push(note(match &listed.error {
            Some(error) => error.clone(),
            None => "loading…".to_string(),
        }));
        return;
    };
    for entry in entries {
        let ignored = under_ignored || entry.ignored;
        if ignored && !show_ignored {
            continue;
        }
        let path = join(folder, &entry.name);
        let is_folder = entry.kind == EntryKind::Folder;
        let open = is_folder && tree.open.contains(&path);
        rows.push(Row {
            depth,
            name: entry.name.clone(),
            path: path.clone(),
            kind: match is_folder {
                true => RowKind::Folder { open },
                false => RowKind::File,
            },
            ignored,
        });
        if open {
            walk(
                tree,
                &path,
                depth + 1,
                show_ignored,
                ignored,
                stale,
                to_ask,
                rows,
            );
        }
    }
}

/// The paths a filter found, laid out as a tree with every folder on the way open.
pub(super) fn filtered_rows(paths: &[String]) -> Vec<Row> {
    let mut sorted: Vec<&String> = paths.iter().collect();
    sorted.sort_by_key(|path| path.to_lowercase());
    let mut shown_folders = HashSet::new();
    let mut rows = Vec::new();
    for path in sorted {
        let parts: Vec<&str> = path.split('/').collect();
        for depth in 0..parts.len() - 1 {
            let folder = parts[..=depth].join("/");
            if shown_folders.insert(folder.clone()) {
                rows.push(Row {
                    depth,
                    name: parts[depth].to_string(),
                    path: folder,
                    kind: RowKind::Folder { open: true },
                    ignored: false,
                });
            }
        }
        rows.push(Row {
            depth: parts.len() - 1,
            name: parts[parts.len() - 1].to_string(),
            path: path.clone(),
            kind: RowKind::File,
            ignored: false,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn described(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                let mark = match row.kind {
                    RowKind::Folder { open: true } => "v ",
                    RowKind::Folder { open: false } => "> ",
                    RowKind::File => "",
                    RowKind::Note => "~ ",
                };
                format!("{}{mark}{}", "  ".repeat(row.depth), row.name)
            })
            .collect()
    }

    #[test]
    fn a_filter_lays_its_paths_out_under_open_folders() {
        let rows = filtered_rows(&[
            "src/native/theme.rs".to_string(),
            "README.md".to_string(),
            "src/lib.rs".to_string(),
        ]);
        assert_eq!(
            described(&rows),
            vec![
                "README.md",
                "v src",
                "  lib.rs",
                "  v native",
                "    theme.rs"
            ]
        );
    }

    #[test]
    fn a_files_ancestors_run_from_the_root_down() {
        assert_eq!(ancestors("src/native/theme.rs"), vec!["src", "src/native"]);
        assert!(ancestors("README.md").is_empty());
    }
}
