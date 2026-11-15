//! Typing that knows about brackets and indentation: automatic closing brackets and quotes, Enter that indents, Tab that
//! uses the file's own indentation, and strict languages (Lisp) that never end up with unbalanced brackets.
use super::{is_word, Buffer, Cursor, Edit};
use crate::lang::{self, closer_of, is_quote, opener_of, Kind};
use ropey::Rope;
use std::sync::Arc;

fn quotes_before(text: &Rope, p: usize, q: char) -> usize {
    let ls = text.line_to_char(text.char_to_line(p));
    text.slice(ls..p).chars().filter(|c| *c == q).count()
}

/// typing `ch` at `p` just steps over the character that is already there
fn skips(text: &Rope, p: usize, ch: char) -> bool {
    p < text.len_chars() && text.char(p) == ch && (opener_of(ch).is_some() || quotes_before(text, p, ch) % 2 == 1)
}

impl Buffer {
    pub fn set_lang(&mut self, l: lang::Lang) {
        if self.lang != l { self.lang = l; self.scan_cache = None; }
    }

    /// the brackets of the text as it is now (matched pairs, unclosed ones); cached until the next edit
    pub fn brackets(&mut self) -> Arc<lang::Scan> {
        if self.loading.is_some() { return Arc::new(lang::Scan::default()); }
        if let Some((v, s)) = &self.scan_cache { if *v == self.version { return s.clone(); } }
        let s = Arc::new(lang::scan(&self.text, &self.lang));
        self.scan_cache = Some((self.version, s.clone()));
        s
    }

    /// Run an edit; in a strict language undo it again when it turned balanced text into unbalanced text.
    pub(crate) fn guarded(&mut self, f: impl FnOnce(&mut Self)) {
        if !self.lang.strict { return f(self); }
        self.ensure_loaded();
        let was_ok = { let s = self.brackets(); s.checked && s.bad.is_empty() };
        let (text, cursors, undo, kind, modified, ver) = (self.text.clone(), self.cursors.clone(), self.undo.len(), self.last_kind, self.modified, self.version);
        f(self);
        if self.version == ver || !was_ok { return; }
        if !self.brackets().bad.is_empty() {
            self.text = text;
            self.cursors = cursors;
            self.undo.truncate(undo);
            (self.last_kind, self.modified) = (kind, modified);
            self.version += 1;
            self.first_changed = 0;
            self.hist_gen += 1;
            self.notice = Some("not done: it would unbalance the brackets".into());
        }
    }

    // ---------------------------------------------------------------- typing
    /// One typed character at every cursor: opening brackets and quotes bring their partner, a closer typed in front of the same
    /// closer steps over it, a selection is wrapped, and a closing bracket alone on its line lines up with its opener.
    pub fn type_char(&mut self, ch: char) {
        self.ensure_loaded();
        let lang = self.lang.clone();
        let opens = closer_of(ch).is_some() && (lang.pairs.contains(ch) || lang.strict);
        let closes = opener_of(ch).is_some();
        let quotes = is_quote(ch) && lang.pairs.contains(ch);
        if !(opens || closes || quotes) { return self.insert(&ch.to_string()); }
        if (closes || quotes) && self.cursors.iter().all(|c| !c.has_sel() && skips(&self.text, c.head, ch)) {
            self.break_undo();
            for c in self.cursors.iter_mut() { *c = Cursor::at(c.head + 1); }
            return;
        }
        let closer = closer_of(ch).unwrap_or(ch);
        self.edit_with("type", &|_, text, c| {
            let (s, e) = (c.start(), c.end());
            let plain = |t: String, head: usize| Edit { start: s, end: e, text: t, head, anchor: head };
            if c.has_sel() {
                if !(opens || quotes) { return plain(ch.to_string(), 1); }
                // wrap, and keep the text selected so that another ( wraps once more
                let inner = text.slice(s..e).to_string();
                return Edit { start: s, end: e, text: format!("{ch}{inner}{closer}"), head: 1 + inner.chars().count(), anchor: 1 };
            }
            let prev = if s > 0 { Some(text.char(s - 1)) } else { None };
            let next = if e < text.len_chars() { Some(text.char(e)) } else { None };
            let word = |c: Option<char>| c.is_some_and(is_word);
            if (closes || quotes) && skips(text, e, ch) { return Edit { start: s, end: e + 1, text: ch.to_string(), head: 1, anchor: 1 }; }
            if opens && (lang.strict || !word(next)) { return plain(format!("{ch}{closer}"), 1); }
            if quotes && !word(prev) && !word(next) && prev != Some(ch) && prev != Some('\\') && quotes_before(text, s, ch) % 2 == 0 {
                return plain(format!("{ch}{ch}"), 1);
            }
            if closes && !matches!(lang.kind, Kind::Lisp | Kind::Prose) {
                // a closer that starts its line goes to the indentation of the line that opened it
                let ls = text.line_to_char(text.char_to_line(s));
                if s > ls && text.slice(ls..s).chars().all(|c| c == ' ' || c == '\t') {
                    if let Some(&(op, _)) = lang::open_at(text, &lang, s).last().filter(|(_, o)| Some(*o) == opener_of(ch)) {
                        let ind = lang::line_indent(text, text.char_to_line(op));
                        let n = ind.chars().count() + 1;
                        return Edit { start: ls, end: e, text: format!("{ind}{ch}"), head: n, anchor: n };
                    }
                }
            }
            plain(ch.to_string(), 1)
        });
    }

    /// Enter: the new line keeps the indentation of the line it leaves, one level more after an opening bracket (or a ':' in
    /// Python and YAML), and a pair `{|}` opens up into three lines. Lisp lines up under the open form.
    pub fn newline(&mut self) {
        let lang = self.lang.clone();
        let unit = lang::indent_unit(&self.text, &lang);
        self.edit_with("newline", &|_, text, c| {
            let (s, e) = (c.start(), c.end());
            let n = |t: String, head: usize| Edit { start: s, end: e, text: t, head, anchor: head };
            if lang.kind == Kind::Lisp {
                let t = format!("\n{}", " ".repeat(lang::lisp_indent(text, &lang, s)));
                let head = t.chars().count();
                return n(t, head);
            }
            let ls = text.line_to_char(text.char_to_line(s));
            let before = text.slice(ls..s).to_string();
            let base: String = before.chars().take_while(|ch| *ch == ' ' || *ch == '\t').collect();
            let last = before.trim_end().chars().last();
            let opens = last.is_some_and(|ch| closer_of(ch).is_some()) || (lang.colon && last == Some(':'));
            let extra = if opens && lang.kind != Kind::Prose { unit.as_str() } else { "" };
            let between = lang.kind != Kind::Prose && s > 0 && e < text.len_chars() && closer_of(text.char(s - 1)) == Some(text.char(e));
            if between {
                let t = format!("\n{base}{unit}\n{base}");
                let head = 1 + base.chars().count() + unit.chars().count();
                return n(t, head);
            }
            let t = format!("\n{base}{extra}");
            let head = t.chars().count();
            n(t, head)
        });
    }

    // ---------------------------------------------------------------- deleting
    /// `(|)`, `"|"`: the characters either side of the caret are a pair that was never filled
    fn empty_pair(&self, p: usize) -> bool {
        if p == 0 || p >= self.len() { return false; }
        let (a, b) = (self.text.char(p - 1), self.text.char(p));
        if is_quote(a) { a == b && self.lang.pairs.contains(a) } else { closer_of(a) == Some(b) && (self.lang.pairs.contains(a) || self.lang.strict) }
    }

    pub fn backspace(&mut self) {
        if self.cursors.iter().all(|c| !c.has_sel()) {
            let scan = if self.lang.strict { Some(self.brackets()) } else { None };
            for i in 0..self.cursors.len() {
                let h = self.cursors[i].head;
                if h == 0 { continue; }
                let c = if self.empty_pair(h) { Cursor { head: h + 1, anchor: h - 1, want: None } }
                    // strict: a real bracket is stepped over, not deleted
                    else if scan.as_ref().is_some_and(|s| s.partner.contains_key(&(h - 1))) { Cursor::at(h - 1) }
                    else { Cursor { head: h, anchor: h - 1, want: None } };
                self.cursors[i] = c;
            }
        }
        self.delete_selected();
    }

    pub fn delete(&mut self) {
        let n = self.len();
        if self.cursors.iter().all(|c| !c.has_sel()) {
            let scan = if self.lang.strict { Some(self.brackets()) } else { None };
            for i in 0..self.cursors.len() {
                let h = self.cursors[i].head;
                if h >= n { continue; }
                let c = if self.lang.strict && self.empty_pair(h) { Cursor { head: h + 1, anchor: h - 1, want: None } }
                    else if scan.as_ref().is_some_and(|s| s.partner.contains_key(&h)) { Cursor::at(h + 1) }
                    else { Cursor { head: h + 1, anchor: h, want: None } };
                self.cursors[i] = c;
            }
        }
        self.delete_selected();
    }

    // ---------------------------------------------------------------- indentation
    /// Tab and Shift-Tab. Tab with no selection inserts one level at the caret (a tab character, or spaces up to the next
    /// multiple of the file's indent width); in Lisp it lines the line up instead. A selection indents all its lines.
    pub fn indent(&mut self, back: bool) {
        let unit = lang::indent_unit(&self.text, &self.lang);
        let lisp = self.lang.kind == Kind::Lisp;
        if !back && !lisp && self.cursors.iter().all(|c| !c.has_sel()) {
            self.edit_with("type", &|_, text, c| {
                let t = if unit == "\t" { "\t".to_string() } else {
                    let ls = text.line_to_char(text.char_to_line(c.start()));
                    let col = crate::app::display_col(&text.slice(ls..c.start()).to_string(), c.start() - ls);
                    " ".repeat(unit.len() - col % unit.len())
                };
                let n = t.chars().count();
                Edit { start: c.start(), end: c.end(), text: t, head: n, anchor: n }
            });
            return;
        }
        self.checkpoint("indent");
        self.normalize();
        let mut lines: Vec<usize> = vec![];
        for c in &self.cursors {
            let (a, b) = (self.text.char_to_line(c.start()), self.text.char_to_line(c.end()));
            for l in a..=b { if !lines.contains(&l) { lines.push(l); } }
        }
        lines.sort();
        let first = lines.first().copied().unwrap_or(0);
        if lisp && !back {
            // top to bottom: a line's indentation depends on the lines above it
            for &l in &lines {
                let st = self.text.line_to_char(l);
                let old = lang::line_indent(&self.text, l).chars().count();
                let new = " ".repeat(lang::lisp_indent(&self.text, &self.lang, st));
                self.text.remove(st..st + old);
                self.text.insert(st, &new);
                let d = new.len() as isize - old as isize;
                for c in self.cursors.iter_mut() {
                    for p in [&mut c.head, &mut c.anchor] { if *p >= st + old { *p = (*p as isize + d) as usize; } else if *p > st { *p = st + new.len(); } }
                }
            }
            self.touched(first);
            return;
        }
        for &l in lines.iter().rev() {
            let st = self.text.line_to_char(l);
            if back {
                let s = self.line_string(l);
                let width = if unit == "\t" { crate::config::tab_width() } else { unit.len() };
                let n = if s.starts_with('\t') { 1 } else { s.chars().take(width).take_while(|c| *c == ' ').count() };
                if n > 0 {
                    self.text.remove(st..st + n);
                    self.shift_after(st, -(n as isize));
                }
            } else {
                self.text.insert(st, &unit);
                self.shift_after(st, unit.chars().count() as isize);
            }
        }
        self.touched(first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn buf(text: &str, lang: &str) -> Buffer {
        let mut b = Buffer::from_text(text, None);
        b.set_lang(Lang::of(lang));
        b
    }
    fn at(b: &mut Buffer, p: usize) { b.cursors = vec![Cursor::at(p)]; }
    fn typed(b: &mut Buffer, s: &str) { for c in s.chars() { b.type_char(c); } }

    #[test]
    fn opener_brings_its_closer_and_the_closer_is_typed_over() {
        let mut b = buf("", "Rust");
        typed(&mut b, "f(a[1], \"x\")");
        assert_eq!(b.contents(), "f(a[1], \"x\")", "typing the whole thing out gives it back, no doubled closers");
        let mut b = buf("", "Rust");
        typed(&mut b, "{(\"");
        assert_eq!(b.contents(), "{(\"\")}");
        assert_eq!(b.primary().head, 3);
        let mut b = buf("", "C");
        typed(&mut b, "'");
        assert_eq!(b.contents(), "''");
    }

    #[test]
    fn pairing_is_careful_with_words_and_apostrophes() {
        let mut b = buf("foo", "C");
        at(&mut b, 3);
        typed(&mut b, "(");
        assert_eq!(b.contents(), "foo()", "after a word it is a call");
        let mut b = buf("", "C");
        at(&mut b, 0);
        b.insert("ab");
        typed(&mut b, "'");
        assert_eq!(b.contents(), "ab'", "don't, it's: an apostrophe after a letter is just that");
        let mut b = buf("x", "C");
        at(&mut b, 0);
        typed(&mut b, "(");
        assert_eq!(b.contents(), "(x", "in front of a word nothing is added");
        let mut b = buf("", "Rust");
        typed(&mut b, "'");
        assert_eq!(b.contents(), "'", "no pairing of ' in Rust (lifetimes)");
        let mut b = buf("", "Plain Text");
        typed(&mut b, "it's (ok)");
        assert_eq!(b.contents(), "it's (ok)");
        let mut b = buf("", "Emacs Lisp");
        typed(&mut b, "'(a b)");
        assert_eq!(b.contents(), "'(a b)", "the Lisp quote is not paired");
        let mut b = buf("a \"b", "Rust");
        at(&mut b, 4);
        typed(&mut b, "\"");
        assert_eq!(b.contents(), "a \"b\"", "closing a string does not open another");
    }

    #[test]
    fn a_selection_is_wrapped_and_stays_selected() {
        let mut b = buf("hello world", "Rust");
        b.cursors = vec![Cursor { head: 5, anchor: 0, want: None }];
        typed(&mut b, "(");
        assert_eq!(b.contents(), "(hello) world");
        typed(&mut b, "\"");
        assert_eq!(b.contents(), "(\"hello\") world");
        b.undo();
        b.undo();
        assert_eq!(b.contents(), "hello world");
        // not an opener: replaced
        b.cursors = vec![Cursor { head: 5, anchor: 0, want: None }];
        typed(&mut b, "x");
        assert_eq!(b.contents(), "x world");
    }

    #[test]
    fn pairs_work_at_every_cursor() {
        let mut b = buf("a\nb", "Rust");
        b.cursors = vec![Cursor::at(1), Cursor::at(3)];
        typed(&mut b, "(");
        assert_eq!(b.contents(), "a()\nb()");
        typed(&mut b, ")");
        assert_eq!(b.contents(), "a()\nb()");
        assert_eq!(b.cursors.iter().map(|c| c.head).collect::<Vec<_>>(), vec![3, 7]);
        // one cursor in front of a closer, one not: the first steps over, the other inserts
        let mut b = buf("x)\ny", "C");
        b.cursors = vec![Cursor::at(1), Cursor::at(4)];
        typed(&mut b, ")");
        assert_eq!(b.contents(), "x)\ny)");
    }

    #[test]
    fn backspace_in_an_empty_pair_removes_both() {
        let mut b = buf("f()", "Rust");
        at(&mut b, 2);
        b.backspace();
        assert_eq!(b.contents(), "f");
        let mut b = buf("f(x)", "Rust");
        at(&mut b, 2);
        b.backspace();
        assert_eq!(b.contents(), "fx)", "a filled pair loses just the opener");
        let mut b = buf("\"\"", "Rust");
        at(&mut b, 1);
        b.backspace();
        assert_eq!(b.contents(), "");
        let mut b = buf("()", "Rust");
        at(&mut b, 1);
        b.delete();
        assert_eq!(b.contents(), "(", "plain Delete removes one character outside strict languages");
    }

    #[test]
    fn enter_indents_and_opens_a_pair() {
        let mut b = buf("fn f() {", "Rust");
        at(&mut b, 8);
        b.newline();
        assert_eq!(b.contents(), "fn f() {\n    ");
        let mut b = buf("    if x {}", "Rust");
        at(&mut b, 10);
        b.newline();
        assert_eq!(b.contents(), "    if x {\n        \n    }");
        assert_eq!(b.primary().head, 10 + 1 + 8);
        let mut b = buf("\tfoo(", "Go");
        at(&mut b, 5);
        b.newline();
        assert_eq!(b.contents(), "\tfoo(\n\t\t");
        let mut b = buf("if x:", "Python");
        at(&mut b, 5);
        b.newline();
        assert_eq!(b.contents(), "if x:\n    ");
        let mut b = buf("  text (", "Plain Text");
        at(&mut b, 8);
        b.newline();
        assert_eq!(b.contents(), "  text (\n  ", "prose keeps the indent and adds none");
    }

    #[test]
    fn indentation_follows_the_file() {
        let mut b = buf("a:\n  b:\n    c: 1\n  d:", "YAML");
        at(&mut b, 21);
        b.newline();
        assert_eq!(b.contents(), "a:\n  b:\n    c: 1\n  d:\n    ", "two-space file, colon opens a level");
        let mut b = buf("fn x() {\n\ty;\n}\n", "Rust");
        at(&mut b, 0);
        b.indent(false);
        assert_eq!(b.contents(), "\tfn x() {\n\ty;\n}\n", "a tab file gets tabs");
        let mut b = buf("x\n  y\n  z\n", "Rust");
        at(&mut b, 0);
        b.indent(false);
        assert_eq!(b.contents(), "  x\n  y\n  z\n");
        b.cursors = vec![Cursor { head: 9, anchor: 0, want: None }];
        b.indent(true);
        assert_eq!(b.contents(), "x\ny\nz\n");
        // spaces up to the next stop
        let mut b = buf("  a b\n  c", "Rust");
        at(&mut b, 3);
        b.indent(false);
        assert_eq!(b.contents(), "  a  b\n  c");
    }

    #[test]
    fn a_closer_on_its_own_line_lines_up_with_its_opener() {
        let mut b = buf("    if x {\n        y;\n        ", "Rust");
        at(&mut b, 31);
        typed(&mut b, "}");
        assert_eq!(b.contents(), "    if x {\n        y;\n    }");
        // not alone on its line: nothing moves
        let mut b = buf("f(\n  a, b", "Rust");
        at(&mut b, 9);
        typed(&mut b, ")");
        assert_eq!(b.contents(), "f(\n  a, b)");
    }

    #[test]
    fn lisp_never_loses_a_bracket() {
        let mut b = buf("(defun f (x) (g x))", "Emacs Lisp");
        at(&mut b, 14);
        b.backspace();                                  // over "(": it steps, it does not delete
        assert_eq!(b.contents(), "(defun f (x) (g x))");
        assert_eq!(b.primary().head, 13);
        b.delete();                                     // and a "(" after the caret is stepped over too
        assert_eq!(b.contents(), "(defun f (x) (g x))");
        assert_eq!(b.primary().head, 14);
        // an empty form can be removed whole
        let mut b = buf("(a ())", "Lisp");
        at(&mut b, 4);
        b.backspace();
        assert_eq!(b.contents(), "(a )");
        // selection: cutting half a form is refused, a whole form is fine
        let mut b = buf("(a (b c))", "Lisp");
        b.cursors = vec![Cursor { head: 6, anchor: 1, want: None }];
        b.insert("");
        assert_eq!(b.contents(), "(a (b c))");
        assert!(b.notice.take().is_some());
        b.cursors = vec![Cursor { head: 8, anchor: 3, want: None }];
        b.insert("");
        assert_eq!(b.contents(), "(a )");
        // a stray closer is not typed
        let mut b = buf("(a)", "Lisp");
        at(&mut b, 1);
        typed(&mut b, ")");
        assert_eq!(b.contents(), "(a)");
        assert!(b.notice.is_some());
        // delete-line and cut respect it as well
        let mut b = buf("(a\nb)", "Lisp");
        at(&mut b, 0);
        b.delete_lines();
        assert_eq!(b.contents(), "(a\nb)");
        // undo does not see the refused edit
        assert!(!b.undo());
    }

    #[test]
    fn strict_can_be_turned_off_and_unbalanced_text_may_be_fixed() {
        let mut l = Lang::of("Lisp");
        l.strict = false;
        let mut b = Buffer::from_text("(a (b))", None);
        b.set_lang(l);
        at(&mut b, 4);
        b.backspace();
        assert_eq!(b.contents(), "(a b))");
        // text that is broken already can be edited freely so it can be repaired
        let mut b = buf("(a (b)", "Lisp");
        at(&mut b, 6);
        typed(&mut b, ")");
        assert_eq!(b.contents(), "(a (b))");
    }

    #[test]
    fn lisp_enter_and_tab_line_up() {
        let mut b = buf("(defun f (x)", "Lisp");
        at(&mut b, 12);
        b.newline();
        assert_eq!(b.contents(), "(defun f (x)\n  ");
        let mut b = buf("(foo bar\nbaz", "Lisp");
        at(&mut b, 12);
        b.indent(false);
        assert_eq!(b.contents(), "(foo bar\n     baz", "Tab lines the line up under the first argument");
        assert_eq!(b.primary().head, 17);
    }

    #[test]
    fn pairing_can_be_configured_away() {
        let mut l = Lang::of("Rust");
        l.pairs.clear();
        let mut b = Buffer::from_text("", None);
        b.set_lang(l);
        typed(&mut b, "(");
        assert_eq!(b.contents(), "(");
    }

    #[test]
    fn brackets_are_scanned_once_per_edit() {
        let mut b = buf("f(a", "Rust");
        let s = b.brackets();
        assert_eq!(s.bad, vec![(1, '(')]);
        assert!(Arc::ptr_eq(&s, &b.brackets()));
        at(&mut b, 3);
        b.insert(")");
        assert!(b.brackets().bad.is_empty());
    }
}
