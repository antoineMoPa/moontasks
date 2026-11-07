//! Back and forward through the files the window has shown, the way a browser goes through
//! pages: one list per window, of the last [`LIMIT`] files.
//!
//! A file counts each time its tab becomes the one in front - opened from the tree, the
//! picker, a jump to a definition, or its tab clicked. Going back moves along the list without
//! adding to it; opening another file after going back drops what was ahead, as a browser does.

/// How many files the list holds. The oldest drops off past it.
pub(crate) const LIMIT: usize = 64;

/// A file that was in front: a session's file as it is now, not an old version of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) session_id: String,
    pub(crate) file_path: String,
}

impl Shown {
    pub(crate) fn name(&self) -> &str {
        crate::native::widgets::file_name_of(&self.file_path)
    }
}

#[derive(Default)]
pub(crate) struct History {
    entries: Vec<Shown>,
    /// Which of `entries` is the one shown, when there are any.
    at: usize,
    /// What was in front on the last frame, so a file only counts as it comes forward rather
    /// than on every frame it stays there. `None` while something other than a file is.
    in_front: Option<Shown>,
}

impl History {
    /// Told every frame what is in front. A file coming forward is a visit - unless it is the
    /// one the list is already at, which is what going back or forward lands on.
    pub(crate) fn saw_in_front(&mut self, shown: Option<Shown>) {
        if shown == self.in_front {
            return;
        }
        self.in_front = shown.clone();
        if let Some(shown) = shown {
            self.visit(shown);
        }
    }

    fn visit(&mut self, shown: Shown) {
        if self.entries.get(self.at) == Some(&shown) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.at + 1);
        }
        self.entries.push(shown);
        if self.entries.len() > LIMIT {
            self.entries.remove(0);
        }
        self.at = self.entries.len() - 1;
    }

    /// The file going back would show.
    pub(crate) fn back(&self) -> Option<&Shown> {
        self.at
            .checked_sub(1)
            .and_then(|index| self.entries.get(index))
    }

    /// The file going forward would show.
    pub(crate) fn forward(&self) -> Option<&Shown> {
        self.entries.get(self.at + 1)
    }

    /// Move one along, back for `-1` and forward for `1`, and say what to show.
    pub(crate) fn step(&mut self, by: isize) -> Option<Shown> {
        let index = self.at.checked_add_signed(by)?;
        self.go_to(index)
    }

    /// Move straight to an entry of [`Self::entries`] - picked from the arrows' list.
    pub(crate) fn go_to(&mut self, index: usize) -> Option<Shown> {
        let shown = self.entries.get(index)?.clone();
        self.at = index;
        Some(shown)
    }

    /// Oldest first.
    pub(crate) fn entries(&self) -> &[Shown] {
        &self.entries
    }

    pub(crate) fn at(&self) -> usize {
        self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> Shown {
        Shown {
            session_id: "s".to_string(),
            file_path: path.to_string(),
        }
    }

    fn paths(history: &History) -> Vec<&str> {
        history
            .entries()
            .iter()
            .map(|shown| shown.file_path.as_str())
            .collect()
    }

    #[test]
    fn a_file_counts_once_as_it_comes_forward() {
        let mut history = History::default();
        history.saw_in_front(Some(file("a")));
        history.saw_in_front(Some(file("a")));
        history.saw_in_front(None);
        history.saw_in_front(Some(file("a")));
        history.saw_in_front(Some(file("b")));
        assert_eq!(paths(&history), vec!["a", "b"]);
        assert_eq!(history.back(), Some(&file("a")));
        assert_eq!(history.forward(), None);
    }

    #[test]
    fn going_back_does_not_add_and_a_new_file_drops_what_was_ahead() {
        let mut history = History::default();
        for path in ["a", "b", "c"] {
            history.saw_in_front(Some(file(path)));
        }
        // Still `c` in front on the frame the step is asked on, then `b` once it is opened.
        assert_eq!(history.step(-1), Some(file("b")));
        history.saw_in_front(Some(file("c")));
        history.saw_in_front(Some(file("b")));
        assert_eq!(paths(&history), vec!["a", "b", "c"]);
        assert_eq!(history.forward(), Some(&file("c")));

        history.saw_in_front(Some(file("d")));
        assert_eq!(paths(&history), vec!["a", "b", "d"]);
        assert_eq!(history.forward(), None);
    }

    #[test]
    fn nothing_to_go_back_to_at_the_start() {
        let mut history = History::default();
        assert_eq!(history.step(-1), None);
        history.saw_in_front(Some(file("a")));
        assert_eq!(history.back(), None);
        assert_eq!(history.step(-1), None);
        assert_eq!(history.step(1), None);
    }

    #[test]
    fn the_oldest_drops_off_past_the_limit() {
        let mut history = History::default();
        for index in 0..LIMIT + 5 {
            history.saw_in_front(Some(file(&index.to_string())));
        }
        assert_eq!(history.entries().len(), LIMIT);
        assert_eq!(history.entries()[0], file("5"));
        assert_eq!(history.at(), LIMIT - 1);
    }
}
