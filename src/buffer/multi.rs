//! Multi-cursor commands: select the next / every occurrence, skip one, drop the newest cursor, one cursor per selected line.
use super::{is_word, Buffer, Cursor};

impl Buffer {
    /// The text the occurrence commands look for: the primary selection. `whole` when it is a complete word (neighbours are not
    /// word characters), so selecting `foo` does not pick up the `foo` inside `foobar`.
    fn occurrence_needle(&self) -> Option<(Vec<char>, bool)> {
        let c = self.primary();
        if !c.has_sel() { return None; }
        let needle: Vec<char> = self.text.slice(c.start()..c.end()).chars().collect();
        let word = needle.iter().all(|&ch| is_word(ch));
        let before = c.start() > 0 && is_word(self.text.char(c.start() - 1));
        let after = c.end() < self.len() && is_word(self.text.char(c.end()));
        Some((needle.clone(), word && !before && !after))
    }

    /// first match of `needle` that starts in `from..to`
    fn find_exact(&self, needle: &[char], whole: bool, from: usize, to: usize) -> Option<usize> {
        let n = needle.len();
        if n == 0 || to < from { return None; }
        let last = to.min(self.len().saturating_sub(n));
        let mut p = from;
        let mut it = self.text.chars_at(from);
        while p <= last {
            let ch = it.next()?;
            if ch == needle[0] && self.text.slice(p..p + n).chars().eq(needle.iter().copied()) {
                let ok = !whole || ((p == 0 || !is_word(self.text.char(p - 1))) && (p + n >= self.len() || !is_word(self.text.char(p + n))));
                if ok { return Some(p); }
            }
            p += 1;
        }
        None
    }

    fn holds_cursor(&self, p: usize, n: usize) -> bool { self.cursors.iter().any(|c| c.start() < p + n && p < c.end() || c.start() == p && c.end() == p + n) }

    /// next match after `from` that no cursor sits on, wrapping around the end of the text
    fn next_free_match(&self, needle: &[char], whole: bool, from: usize) -> Option<usize> {
        let n = needle.len();
        let mut at = from;
        let mut wrapped = false;
        loop {
            let hit = match self.find_exact(needle, whole, at, self.len()) {
                Some(p) => Some(p),
                None if !wrapped => { wrapped = true; at = 0; self.find_exact(needle, whole, 0, from) }
                None => None,
            };
            let p = hit?;
            if wrapped && p >= from && at == 0 && p + n > from + n { return None; }
            if !self.holds_cursor(p, n) { return Some(p); }
            at = p + 1;
            if at > self.len() { if wrapped { return None; } wrapped = true; at = 0; }
            if wrapped && at >= from + n { return None; }
        }
    }

    /// Ctrl-D: with no selection select the word under the caret, else add a cursor on the next occurrence of the selected text.
    pub fn add_next_occurrence(&mut self) -> bool {
        self.ensure_loaded();
        let last = self.primary();
        if !last.has_sel() {
            let (a, b) = self.word_at(last.head);
            if a == b { return false; }
            *self.cursors.last_mut().unwrap() = Cursor { head: b, anchor: a, want: None };
            return true;
        }
        let Some((needle, whole)) = self.occurrence_needle() else { return false };
        match self.next_free_match(&needle, whole, last.end()) {
            Some(p) => { self.cursors.push(Cursor { head: p + needle.len(), anchor: p, want: None }); true }
            None => false,
        }
    }

    /// Skip: drop the newest occurrence cursor and take the next one instead (one cursor: move the selection on).
    pub fn skip_occurrence(&mut self) -> bool {
        self.ensure_loaded();
        let Some((needle, whole)) = self.occurrence_needle() else { return false };
        let last = self.primary();
        let kept = if self.cursors.len() > 1 { self.cursors.pop() } else { None };
        let from = last.end();
        let p = if kept.is_some() { self.next_free_match(&needle, whole, from) } else {
            // alone: look for the next match other than the current one
            self.next_free_match(&needle, whole, from).filter(|&p| p != last.start())
        };
        match p {
            Some(p) => {
                let c = Cursor { head: p + needle.len(), anchor: p, want: None };
                if kept.is_none() { self.cursors = vec![c]; } else { self.cursors.push(c); }
                true
            }
            None => { if let Some(k) = kept { self.cursors.push(k); } false }
        }
    }

    /// Select every occurrence of the selected text (or of the word under the caret). Returns how many cursors there are.
    pub fn select_all_occurrences(&mut self) -> usize {
        self.ensure_loaded();
        if !self.primary().has_sel() && !self.add_next_occurrence() { return 0; }
        let Some((needle, whole)) = self.occurrence_needle() else { return 0 };
        let cur = self.primary().start();
        let (mut all, mut at) = (vec![], 0);
        while let Some(p) = self.find_exact(&needle, whole, at, self.len()) {
            all.push(Cursor { head: p + needle.len(), anchor: p, want: None });
            at = p + needle.len();
        }
        // the occurrence you started from stays the primary cursor
        if let Some(i) = all.iter().position(|c| c.start() == cur) { let c = all.remove(i); all.push(c); }
        self.cursors = all;
        self.normalize();
        self.cursors.len()
    }

    /// What every cursor has selected, in text order, when there are several cursors and all of them select something
    /// (so a later paste at as many cursors can hand each its own piece back).
    pub fn selection_parts(&self) -> Vec<String> {
        if self.cursors.len() < 2 || self.cursors.iter().any(|c| !c.has_sel()) { return vec![]; }
        let mut cs = self.cursors.clone();
        cs.sort_by_key(|c| c.start());
        cs.iter().map(|c| self.text.slice(c.start()..c.end()).to_string()).collect()
    }

    /// Ctrl-U: forget the newest cursor; the one before it (by position) becomes the primary. False with a single cursor.
    pub fn drop_newest_cursor(&mut self) -> bool {
        if self.cursors.len() < 2 { return false; }
        let gone = self.cursors.pop().unwrap();
        let prev = self.cursors.iter().rposition(|c| c.start() < gone.start()).unwrap_or(self.cursors.len() - 1);
        let c = self.cursors.remove(prev);
        self.cursors.push(c);
        true
    }

    /// Alt-L: a multi-line selection becomes one cursor at the end of each of its lines. Returns how many cursors there are,
    /// 0 when no selection spans lines.
    pub fn cursors_at_line_ends(&mut self) -> usize {
        if !self.cursors.iter().any(|c| self.text.char_to_line(c.start()) != self.text.char_to_line(c.end())) { return 0; }
        let mut out = vec![];
        for c in &self.cursors {
            let (a, b) = (self.text.char_to_line(c.start()), self.text.char_to_line(c.end()));
            if a == b { out.push(*c); continue; }
            // a selection that ends at the start of a line does not include that line
            let b = if c.end() == self.text.line_to_char(b) { b - 1 } else { b };
            for l in a..=b {
                let end = self.idx(l, self.line_len(l));
                out.push(Cursor::at(if l == b { end.min(c.end()).max(c.start()) } else { end }));
            }
        }
        self.cursors = out;
        self.normalize();
        self.cursors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Buffer { Buffer::from_text(s, None) }
    fn heads(x: &Buffer) -> Vec<usize> { x.cursors.iter().map(|c| c.head).collect() }
    fn sel(from: usize, to: usize) -> Cursor { Cursor { head: to, anchor: from, want: None } }

    #[test]
    fn next_occurrence_is_whole_word_when_you_started_from_a_word() {
        let mut x = b("foo foobar foo");
        x.cursors = vec![Cursor::at(1)];
        assert!(x.add_next_occurrence());                       // the word under the caret
        assert!(x.add_next_occurrence());                       // skips the foo inside foobar
        assert_eq!(x.cursors.iter().map(|c| (c.start(), c.end())).collect::<Vec<_>>(), vec![(0, 3), (11, 14)]);
        assert!(!x.add_next_occurrence());
        // a selection that is only a part of a word matches parts of words
        let mut y = b("foo foobar foo");
        y.cursors = vec![sel(0, 2)];
        assert!(y.add_next_occurrence());
        assert_eq!(y.primary().start(), 4);
    }

    #[test]
    fn next_occurrence_wraps_around_and_stops_when_all_are_taken() {
        let mut x = b("ab x ab y ab");
        x.cursors = vec![sel(5, 7)];                            // the middle one
        assert!(x.add_next_occurrence());                       // -> the last
        assert!(x.add_next_occurrence());                       // wraps to the first
        assert!(!x.add_next_occurrence());
        assert_eq!(x.cursors.len(), 3);
        x.insert("Z");
        assert_eq!(x.contents(), "Z x Z y Z");
    }

    #[test]
    fn multi_line_selection_is_found_again() {
        let mut x = b("a\nb\nc\na\nb\nd");
        x.cursors = vec![sel(0, 3)];
        assert!(x.add_next_occurrence());
        assert_eq!(x.cursors.len(), 2);
        assert_eq!(x.primary().start(), 6);
    }

    #[test]
    fn select_all_occurrences_keeps_the_current_one_primary() {
        let mut x = b("foo bar foo baz foo foobar");
        x.cursors = vec![Cursor::at(9)];                        // inside the middle foo
        assert_eq!(x.select_all_occurrences(), 3);
        assert_eq!(x.primary().start(), 8);
        x.insert("X");
        assert_eq!(x.contents(), "X bar X baz X foobar");
        let mut y = b("   ");
        assert_eq!(y.select_all_occurrences(), 0, "nothing to select on blanks");
    }

    #[test]
    fn skip_drops_the_newest_cursor_and_takes_the_next_one() {
        let mut x = b("a1 a2 a3 a4");
        x.cursors = vec![sel(0, 1)];
        // selection is "a" which is a part of a word: matches every a
        assert!(x.add_next_occurrence());                       // 0 and 3
        assert!(x.skip_occurrence());                           // 0 and 6 (3 skipped)
        let mut at: Vec<usize> = x.cursors.iter().map(|c| c.start()).collect();
        at.sort();
        assert_eq!(at, vec![0, 6]);
        // alone: the selection moves on
        let mut y = b("a1 a2 a3");
        y.cursors = vec![sel(0, 1)];
        assert!(y.skip_occurrence());
        assert_eq!((y.cursors.len(), y.primary().start()), (1, 3));
        // nothing selected: nothing to skip
        let mut z = b("abc");
        assert!(!z.skip_occurrence());
    }

    #[test]
    fn drop_newest_cursor_steps_back() {
        let mut x = b("w w w w");
        x.cursors = vec![sel(0, 1)];
        for _ in 0..3 { assert!(x.add_next_occurrence()); }
        assert_eq!(x.cursors.len(), 4);
        assert!(x.drop_newest_cursor());
        assert_eq!(x.cursors.len(), 3);
        assert_eq!(x.primary().start(), 4, "the one before becomes primary");
        assert!(x.drop_newest_cursor() && x.drop_newest_cursor());
        assert!(!x.drop_newest_cursor(), "the last cursor stays");
        assert_eq!(x.cursors.len(), 1);
    }

    #[test]
    fn selected_lines_become_a_cursor_per_line() {
        let mut x = b("one\ntwo\nthree\nfour");
        x.cursors = vec![sel(1, 11)];                           // from "ne" to inside "three"
        assert_eq!(x.cursors_at_line_ends(), 3);
        assert_eq!(heads(&x), vec![3, 7, 11]);
        x.insert(";");
        assert_eq!(x.contents(), "one;\ntwo;\nthr;ee\nfour");
        // whole lines selected including the final newline: no cursor on the line after
        let mut y = b("a\nb\nc");
        y.cursors = vec![sel(0, 4)];
        assert_eq!(y.cursors_at_line_ends(), 2);
        assert_eq!(heads(&y), vec![1, 3]);
        let mut z = b("abc");
        assert_eq!(z.cursors_at_line_ends(), 0);
    }

    #[test]
    fn every_edit_works_at_every_cursor() {
        // typing, backspace, delete, delete word, newline with indentation, tab
        let mut x = b("  ab cd\n  ef gh");
        x.cursors = vec![Cursor::at(3), Cursor::at(11)];
        x.insert("X");
        assert_eq!(x.contents(), "  aXb cd\n  eXf gh");
        x.backspace();
        assert_eq!(x.contents(), "  ab cd\n  ef gh");
        x.delete();
        assert_eq!(x.contents(), "  a cd\n  e gh");
        x.delete_word_back();
        assert_eq!(x.contents(), "   cd\n   gh");
        x.newline();
        assert_eq!(x.contents(), "  \n   cd\n  \n   gh", "each cursor keeps the indent of its own line");
        let mut y = b("a\nb");
        y.cursors = vec![Cursor::at(1), Cursor::at(3)];
        y.indent(false);
        assert_eq!(y.contents(), "a   \nb   ", "Tab goes to the next stop");
        y.undo();
        assert_eq!(y.contents(), "a\nb");
        assert_eq!(heads(&y), vec![1, 3], "undo brings the cursors back");
    }

    #[test]
    fn selections_are_replaced_one_by_one() {
        let mut x = b("one two three");
        x.cursors = vec![sel(0, 3), sel(8, 13)];
        x.insert("#");
        assert_eq!(x.contents(), "# two #");
        assert_eq!(heads(&x), vec![1, 7]);
        let mut y = b("one two three");
        y.cursors = vec![sel(0, 3), sel(8, 13)];
        assert_eq!(y.cut(), "one\nthree");
        assert_eq!(y.contents(), " two ");
    }

    #[test]
    fn cursors_that_collide_are_merged() {
        // moving
        let mut x = b("ab\ncd");
        x.cursors = vec![Cursor::at(0), Cursor::at(1)];
        x.move_cursors(super::super::Move::Left, false);
        assert_eq!(x.cursors.len(), 1);
        // deleting: two carets side by side, both backspace
        let mut y = b("ab");
        y.cursors = vec![Cursor::at(1), Cursor::at(2)];
        y.backspace();
        assert_eq!(y.contents(), "");
        assert_eq!(y.cursors.len(), 1);
        y.insert("z");
        assert_eq!(y.contents(), "z", "a merged cursor types once");
        // on the same line from different lines moving up
        let mut w = b("abc\nabc");
        w.cursors = vec![Cursor::at(1), Cursor::at(5)];
        w.move_cursors(super::super::Move::Up, false);
        w.move_cursors(super::super::Move::Up, false);
        assert_eq!(w.cursors.len(), 1);
        // adding a cursor above/below onto an existing one does nothing new
        let mut v = b("a\nb\nc");
        v.cursors = vec![Cursor::at(0)];
        v.add_cursor_vertical(true);
        v.add_cursor_vertical(true);
        assert_eq!(heads(&v).len(), 3);
        v.add_cursor_vertical(true);
        assert_eq!(v.cursors.len(), 3, "no cursor past the last line");
    }

    #[test]
    fn line_commands_with_several_cursors() {
        let mut x = b("a\nb\nc\nd");
        x.cursors = vec![Cursor::at(0), Cursor::at(1), Cursor::at(4)];   // two on line 1
        x.duplicate_lines();
        assert_eq!(x.contents(), "a\na\nb\nc\nc\nd");
        let mut y = b("a\nb\nc\nd");
        y.cursors = vec![sel(0, 3), Cursor::at(2)];                       // overlapping ranges
        y.duplicate_lines();
        assert_eq!(y.contents(), "a\nb\na\nb\nc\nd");
        let mut z = b("a\nb\nc\nd");
        z.cursors = vec![Cursor::at(0), Cursor::at(4), Cursor::at(6)];
        assert_eq!(z.cut(), "a\nc\nd\n", "cutting lines takes the line of every cursor");
        assert_eq!(z.contents(), "b\n");
    }

    #[test]
    fn per_cursor_clipboard_pieces_even_with_newlines_inside() {
        let mut x = b("1 2");
        x.cursors = vec![Cursor::at(1), Cursor::at(3)];
        x.paste_parts(&["a\nb".to_string(), "c".to_string()]);
        assert_eq!(x.contents(), "1a\nb 2c");
    }

    #[test]
    fn many_cursors_edit_in_linear_time() {
        let text = "x ".repeat(50_000);
        let mut x = b(&text);
        x.cursors = vec![sel(0, 1)];
        assert_eq!(x.select_all_occurrences(), 50_000);
        let t = std::time::Instant::now();
        x.insert("yy");
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
        assert!(x.contents().starts_with("yy yy yy"));
        assert_eq!(x.cursors.len(), 50_000);
    }

    #[test]
    fn nothing_to_delete_is_not_an_edit() {
        let mut x = b("a");
        x.backspace();   // at 0
        assert!(!x.modified);
        x.cursors = vec![Cursor::at(1)];
        x.delete();      // at the end
        assert!(!x.modified && !x.undo());
    }

    #[test]
    fn alt_click_on_a_cursor_removes_it() {
        let mut x = b("abc");
        x.click(1, false, true);
        x.click(2, false, true);
        assert_eq!(x.cursors.len(), 3);
        x.click(2, false, true);
        assert_eq!(x.cursors.len(), 2);
        x.click(0, false, true);
        assert_eq!(x.cursors.len(), 1, "never below one");
    }
}
