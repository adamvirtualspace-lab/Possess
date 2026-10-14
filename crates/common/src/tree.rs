//! Turning the API's flat folder map into the hierarchy the sidebar renders.
//!
//! `/api/folders` reports only folders that *directly* hold notes, keyed by
//! their path relative to the vault: `{"." : [...], "a": [...], "a/b/c": [...]}`.
//! Splitting those keys rebuilds the real tree — and an intermediate folder that
//! holds no notes of its own still appears, because it shows up as a segment on
//! the way down to one that does.
//!
//! Kept here, beside the response type it consumes, so it can be tested without
//! a browser.

use crate::NoteEntry;
use std::collections::BTreeMap;

/// One note in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRef {
    pub name: String,
    /// Vault-relative path, which is what every other endpoint takes.
    pub path: String,
}

/// A folder and everything directly inside it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    /// Vault-relative path of this folder; empty for the root.
    pub path: String,
    pub folders: Vec<TreeNode>,
    pub notes: Vec<NoteRef>,
}

impl TreeNode {
    fn new(name: &str, path: &str) -> Self {
        Self {
            name: name.to_string(),
            path: path.to_string(),
            folders: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// The child folder called `name`, created if this is the first time it has
    /// been seen.
    fn child(&mut self, name: &str) -> &mut TreeNode {
        let path = if self.path.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.path)
        };

        // Linear, but a folder's children are few and this runs once per
        // rebuild; a map here would cost more in ordering than it saves.
        if let Some(index) = self.folders.iter().position(|f| f.name == name) {
            return &mut self.folders[index];
        }
        self.folders.push(TreeNode::new(name, &path));
        self.folders.last_mut().expect("just pushed")
    }

    /// Sort this node and everything under it: folders first, then notes, each
    /// group by name.
    fn sort(&mut self) {
        self.folders.sort_by(|a, b| compare_names(&a.name, &b.name));
        self.notes.sort_by(|a, b| compare_names(&a.name, &b.name));
        for folder in &mut self.folders {
            folder.sort();
        }
    }

    /// Every note under this node, depth first — the flat list wiki links and
    /// the autocomplete resolve against.
    pub fn note_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        self.collect_notes(&mut paths);
        paths.sort();
        paths
    }

    fn collect_notes(&self, into: &mut Vec<String>) {
        into.extend(self.notes.iter().map(|note| note.path.clone()));
        for folder in &self.folders {
            folder.collect_notes(into);
        }
    }
}

/// Case-insensitive first, so "Zebra" and "apple" sort the way a reader expects;
/// exact comparison breaks ties so the order is never arbitrary.
fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
}

/// Build the tree from the map `/api/folders` returns.
pub fn build(folders: &BTreeMap<String, Vec<NoteEntry>>) -> TreeNode {
    let mut root = TreeNode::new("", "");

    for (folder, entries) in folders {
        let node = if folder == "." {
            &mut root
        } else {
            // Walking the segments is what makes an intermediate folder appear
            // even when it holds no notes itself.
            folder
                .split('/')
                .filter(|segment| !segment.is_empty())
                .fold(&mut root, |node, segment| node.child(segment))
        };

        for entry in entries {
            let name = entry.0.clone();
            let path = if folder == "." {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
            node.notes.push(NoteRef { name, path });
        }
    }

    root.sort();
    root
}

/// The folder paths above `path`, outermost first.
///
/// Selecting a note deep in the tree has to open every folder on the way down to
/// it, which is what this lists.
pub fn ancestors(path: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut current = String::new();

    // The last segment is the note itself, so it is not an ancestor.
    for segment in path.split('/').rev().skip(1).collect::<Vec<_>>().iter().rev() {
        if segment.is_empty() {
            continue;
        }
        current = if current.is_empty() {
            (*segment).to_string()
        } else {
            format!("{current}/{segment}")
        };
        found.push(current.clone());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> NoteEntry {
        NoteEntry(name.to_string(), format!("/abs/{name}"))
    }

    fn folders(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<NoteEntry>> {
        pairs
            .iter()
            .map(|(folder, names)| {
                (folder.to_string(), names.iter().map(|n| entry(n)).collect())
            })
            .collect()
    }

    #[test]
    fn the_root_key_holds_notes_at_the_vault_root() {
        let tree = build(&folders(&[(".", &["b.md", "a.md"])]));
        assert!(tree.folders.is_empty());
        assert_eq!(
            tree.notes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["a.md", "b.md"]
        );
        // A root note's path is just its name — no "./" prefix.
        assert_eq!(tree.notes[0].path, "a.md");
    }

    #[test]
    fn nests_folders_and_builds_vault_relative_paths() {
        let tree = build(&folders(&[("a/b", &["deep.md"])]));

        let a = &tree.folders[0];
        assert_eq!((a.name.as_str(), a.path.as_str()), ("a", "a"));

        let b = &a.folders[0];
        assert_eq!((b.name.as_str(), b.path.as_str()), ("b", "a/b"));
        assert_eq!(b.notes[0].path, "a/b/deep.md");
    }

    #[test]
    fn an_intermediate_folder_with_no_notes_still_appears() {
        // "a" holds nothing itself; it exists only as a segment on the way to
        // "a/b/c". Without it the tree would have nowhere to hang "b".
        let tree = build(&folders(&[("a/b/c", &["deep.md"])]));

        assert_eq!(tree.folders.len(), 1);
        let a = &tree.folders[0];
        assert_eq!(a.name, "a");
        assert!(a.notes.is_empty());

        let b = &a.folders[0];
        assert_eq!(b.name, "b");
        assert!(b.notes.is_empty());

        assert_eq!(b.folders[0].name, "c");
        assert_eq!(b.folders[0].notes[0].path, "a/b/c/deep.md");
    }

    #[test]
    fn folders_sharing_a_parent_become_siblings() {
        let tree = build(&folders(&[("a/x", &["one.md"]), ("a/y", &["two.md"])]));
        let a = &tree.folders[0];
        assert_eq!(
            a.folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["x", "y"]
        );
    }

    #[test]
    fn sorts_case_insensitively_at_every_level() {
        let tree = build(&folders(&[
            (".", &["Zebra.md", "apple.md"]),
            ("Beta", &["one.md"]),
            ("alpha", &["two.md"]),
        ]));

        assert_eq!(
            tree.folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "Beta"]
        );
        assert_eq!(
            tree.notes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["apple.md", "Zebra.md"]
        );
    }

    #[test]
    fn an_empty_vault_is_an_empty_root() {
        let tree = build(&BTreeMap::new());
        assert!(tree.folders.is_empty() && tree.notes.is_empty());
        assert!(tree.note_paths().is_empty());
    }

    #[test]
    fn note_paths_flattens_the_whole_tree() {
        let tree = build(&folders(&[
            (".", &["root.md"]),
            ("a", &["one.md"]),
            ("a/b", &["two.md"]),
        ]));
        assert_eq!(tree.note_paths(), ["a/b/two.md", "a/one.md", "root.md"]);
    }

    #[test]
    fn ancestors_lists_the_folders_on_the_way_down() {
        assert_eq!(ancestors("a/b/c/note.md"), ["a", "a/b", "a/b/c"]);
        assert_eq!(ancestors("a/note.md"), ["a"]);
        // A note at the root has none.
        assert!(ancestors("note.md").is_empty());
    }
}
