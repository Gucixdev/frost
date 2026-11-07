//! Text buffer: a rope, several cursors, snapshot undo (a Rope clone is O(1), so a snapshot is cheap).
use ropey::Rope;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub head: usize,   // char index where the caret is
    pub anchor: usize, // other end of the selection (== head when nothing is selected)
    pub want: Option<usize>, // column kept while moving up/down over short lines
}

impl Cursor {
    pub fn at(p: usize) -> Self { Cursor { head: p, anchor: p, want: None } }
    pub fn start(&self) -> usize { self.head.min(self.anchor) }
    pub fn end(&self) -> usize { self.head.max(self.anchor) }
    pub fn has_sel(&self) -> bool { self.head != self.anchor }
}

/// One undo step stored compactly: replace `del` chars at `start` by `ins`.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Delta { pub start: usize, pub del: usize, pub ins: String }

/// The whole undo/redo history in a form that survives a restart: the oldest text plus the steps after it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct History { pub base: String, pub steps: Vec<Delta>, pub current: usize }

/// the single replacement that turns `a` into `b` (common head and tail trimmed)
pub fn delta(a: &str, b: &str) -> Delta {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    let mut p = ab.iter().zip(bb).take_while(|(x, y)| x == y).count();
    while p > 0 && !(a.is_char_boundary(p) && b.is_char_boundary(p)) { p -= 1; }
    let max = ab.len().min(bb.len()) - p;
    let mut s = ab.iter().rev().zip(bb.iter().rev()).take(max).take_while(|(x, y)| x == y).count();
    while s > 0 && !(a.is_char_boundary(ab.len() - s) && b.is_char_boundary(bb.len() - s)) { s -= 1; }
    Delta { start: a[..p].chars().count(), del: a[p..ab.len() - s].chars().count(), ins: b[p..bb.len() - s].to_string() }
}

fn apply(r: &mut Rope, d: &Delta) {
    let n = r.len_chars();
    let s = d.start.min(n);
    r.remove(s..(s + d.del).min(n));
    r.insert(s, &d.ins);
}

/// history is only kept on disk for texts up to this size and this many steps
const HIST_MAX_CHARS: usize = 4 << 20;
const HIST_MAX_STEPS: usize = 50_000;

#[derive(Clone)]
struct Snap { text: Rope, cursors: Vec<Cursor> }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Move { Left, Right, Up, Down, Home, End, WordLeft, WordRight, PageUp(usize), PageDown(usize), DocStart, DocEnd }

pub struct Buffer {
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub crlf: bool,
    pub cursors: Vec<Cursor>,
    pub syntax: usize,
    pub scroll: usize,
    pub hscroll: usize,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    last_kind: &'static str,
    /// bumped on every edit: the highlighter cache and the session know when to refresh
    pub version: u64,
    pub first_changed: usize,
    pub id: u64,
    pub loading: Option<Loader>,
    /// shown instead of the file name (diff views)
    pub title: Option<String>,
}

fn is_word(c: char) -> bool { c.is_alphanumeric() || c == '_' }

impl Buffer {
    pub fn new() -> Self {
        Buffer { text: Rope::new(), path: None, modified: false, crlf: false, cursors: vec![Cursor::at(0)], syntax: 0, scroll: 0, hscroll: 0,
                 undo: vec![], redo: vec![], last_kind: "", version: 0, first_changed: usize::MAX, loading: None, title: None,
                 id: { static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1); N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) } }
    }

    pub fn from_text(s: &str, path: Option<PathBuf>) -> Self {
        let crlf = s.contains("\r\n");
        let mut b = Buffer::new();
        b.text = Rope::from_str(&if crlf { s.replace("\r\n", "\n") } else { s.to_string() });
        b.crlf = crlf;
        b.path = path;
        b
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        use std::io::Write;
        self.ensure_loaded();
        let p = self.path.clone().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no file name"))?;
        let tmp = p.with_extension("frost-tmp");
        {
            let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&tmp)?);
            for chunk in self.text.chunks() {
                if self.crlf {
                    let mut it = chunk.split('\n');
                    if let Some(first) = it.next() { w.write_all(first.as_bytes())?; }
                    for part in it { w.write_all(b"\r\n")?; w.write_all(part.as_bytes())?; }
                } else { w.write_all(chunk.as_bytes())?; }
            }
            w.flush()?;
        }
        std::fs::rename(&tmp, &p)?;
        self.modified = false;
        Ok(())
    }

    pub fn contents(&self) -> String { self.text.to_string() }
    pub fn len(&self) -> usize { self.text.len_chars() }
    /// number of lines the way an editor counts them: the empty last line after a final "\n" counts
    pub fn lines(&self) -> usize { self.text.len_lines() }
    pub fn line_string(&self, i: usize) -> String {
        if i >= self.lines() { return String::new(); }
        let mut s = self.text.line(i).to_string();
        while s.ends_with('\n') || s.ends_with('\r') { s.pop(); }
        s
    }
    pub fn line_len(&self, i: usize) -> usize { self.line_string(i).chars().count() }
    pub fn pos(&self, p: usize) -> (usize, usize) {
        let p = p.min(self.len());
        let l = self.text.char_to_line(p);
        (l, p - self.text.line_to_char(l))
    }
    pub fn idx(&self, line: usize, col: usize) -> usize {
        let line = line.min(self.lines().saturating_sub(1));
        self.text.line_to_char(line) + col.min(self.line_len(line))
    }
    pub fn primary(&self) -> Cursor { *self.cursors.last().unwrap_or(&Cursor::at(0)) }
    pub fn name(&self) -> String {
        if let Some(t) = &self.title { return t.clone(); }
        self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled".into())
    }

    // ---------------------------------------------------------------- undo
    fn checkpoint(&mut self, kind: &'static str) {
        self.ensure_loaded();
        let coalesce = kind == "type" && self.last_kind == "type";
        if !coalesce {
            self.undo.push(Snap { text: self.text.clone(), cursors: self.cursors.clone() });
        }
        self.redo.clear();
        self.last_kind = kind;
    }
    /// replace the `n` chars before the single caret with `with` (accepting a completion)
    pub fn replace_before_caret(&mut self, n: usize, with: &str) {
        if self.cursors.len() != 1 || self.cursors[0].has_sel() { return; }
        self.checkpoint("complete");
        let h = self.cursors[0].head;
        let s = h.saturating_sub(n);
        self.text.remove(s..h);
        self.text.insert(s, with);
        self.cursors = vec![Cursor::at(s + with.chars().count())];
        self.touched(self.text.char_to_line(s));
        self.last_kind = "";
    }

    /// the word touching the primary caret and where it starts (char index)
    pub fn word_at_caret(&self) -> (String, usize) {
        let h = self.cursors[0].head;
        let mut s = h;
        while s > 0 && is_word(self.text.char(s - 1)) { s -= 1; }
        let mut e = h;
        while e < self.len() && is_word(self.text.char(e)) { e += 1; }
        (self.text.slice(s..e).to_string(), s)
    }

    /// distinct words of 4+ letters in the lines around the caret, nearest first
    pub fn nearby_words(&self, radius: usize) -> Vec<String> {
        let l = self.text.char_to_line(self.cursors[0].head);
        let (a, z) = (l.saturating_sub(radius), (l + radius).min(self.lines()));
        let mut out: Vec<String> = vec![];
        let mut order: Vec<usize> = (a..z).collect();
        order.sort_by_key(|&i| i.abs_diff(l));
        for i in order {
            for w in self.text.line(i).to_string().split(|c: char| !is_word(c)) {
                if w.chars().count() >= 4 && !out.iter().any(|o| o == w) { out.push(w.to_string()); }
            }
            if out.len() > 400 { break; }
        }
        out
    }

    /// The full history for the session file, or None when there is none or it is too big to be worth saving.
    pub fn history(&self) -> Option<History> {
        if self.loading.is_some() || self.undo.is_empty() && self.redo.is_empty() { return None; }
        if self.len() > HIST_MAX_CHARS || self.undo.len() + self.redo.len() > HIST_MAX_STEPS { return None; }
        let mut texts: Vec<String> = self.undo.iter().map(|s| s.text.to_string()).collect();
        texts.push(self.text.to_string());
        texts.extend(self.redo.iter().rev().map(|s| s.text.to_string()));
        let steps = texts.windows(2).map(|w| delta(&w[0], &w[1])).collect();
        Some(History { base: texts.swap_remove(0), steps, current: self.undo.len() })
    }

    /// Rebuild undo/redo from a saved history. Refuses (and changes nothing) when it does not end in the text we have now.
    pub fn set_history(&mut self, h: &History) -> bool {
        if self.loading.is_some() || h.current > h.steps.len() { return false; }
        let mut cur = Rope::from_str(&h.base);
        let mut ropes = vec![cur.clone()];
        let mut ends = vec![0usize];
        for d in &h.steps { apply(&mut cur, d); ends.push(d.start + d.ins.chars().count()); ropes.push(cur.clone()); }
        if ropes[h.current] != self.text { return false; }
        let snap = |i: usize| Snap { text: ropes[i].clone(), cursors: vec![Cursor::at(ends[i].min(ropes[i].len_chars()))] };
        self.undo = (0..h.current).map(snap).collect();
        self.redo = (h.current + 1..ropes.len()).rev().map(snap).collect();
        true
    }

    pub fn break_undo(&mut self) { self.last_kind = ""; }
    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snap { text: self.text.clone(), cursors: self.cursors.clone() });
            self.restore(s);
        }
    }
    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snap { text: self.text.clone(), cursors: self.cursors.clone() });
            self.restore(s);
        }
    }
    fn restore(&mut self, s: Snap) {
        self.text = s.text;
        self.cursors = s.cursors;
        self.modified = true;
        self.version += 1;
        self.first_changed = 0;
        self.last_kind = "";
    }

    // ---------------------------------------------------------------- editing at every cursor
    /// Replace every cursor's selection with f(selected text) -> (new text). Cursors end after the new text.
    fn edit_each(&mut self, kind: &'static str, f: &dyn Fn(&str) -> String) {
        self.checkpoint(kind);
        self.normalize();
        let mut cs = self.cursors.clone();
        let mut first = usize::MAX;
        for i in (0..cs.len()).rev() {
            let (s, e) = (cs[i].start(), cs[i].end());
            let new = f(&self.text.slice(s..e).to_string());
            first = first.min(self.text.char_to_line(s));
            self.text.remove(s..e);
            self.text.insert(s, &new);
            let n = new.chars().count();
            let delta = n as isize - (e - s) as isize;
            cs[i] = Cursor::at(s + n);
            for c in cs.iter_mut().skip(i + 1) {
                c.head = (c.head as isize + delta) as usize;
                c.anchor = (c.anchor as isize + delta) as usize;
            }
        }
        self.cursors = cs;
        self.touched(first);
    }

    fn touched(&mut self, first_line: usize) {
        self.modified = true;
        self.version += 1;
        self.first_changed = self.first_changed.min(if first_line == usize::MAX { 0 } else { first_line });
    }

    pub fn insert(&mut self, s: &str) {
        let s = s.to_string();
        self.edit_each("type", &move |_| s.clone());
    }

    /// Paste: with as many clipboard lines as cursors each cursor gets one line, otherwise everyone gets the whole text.
    pub fn paste(&mut self, s: &str) {
        let parts: Vec<String> = s.lines().map(|l| l.to_string()).collect();
        if self.cursors.len() > 1 && parts.len() == self.cursors.len() {
            self.checkpoint("paste");
            self.normalize();
            let mut cs = self.cursors.clone();
            let mut first = usize::MAX;
            for i in (0..cs.len()).rev() {
                let (a, b) = (cs[i].start(), cs[i].end());
                first = first.min(self.text.char_to_line(a));
                self.text.remove(a..b);
                self.text.insert(a, &parts[i]);
                let n = parts[i].chars().count();
                let delta = n as isize - (b - a) as isize;
                cs[i] = Cursor::at(a + n);
                for c in cs.iter_mut().skip(i + 1) { c.head = (c.head as isize + delta) as usize; c.anchor = (c.anchor as isize + delta) as usize; }
            }
            self.cursors = cs;
            self.touched(first);
        } else {
            let s = s.replace("\r\n", "\n");
            self.edit_each("paste", &move |_| s.clone());
        }
    }

    pub fn newline(&mut self) {
        // keep the indentation of the line the cursor is on
        let ind: Vec<String> = self.cursors.iter().map(|c| {
            let l = self.text.char_to_line(c.start());
            self.line_string(l).chars().take_while(|ch| *ch == ' ' || *ch == '\t').collect()
        }).collect();
        if self.cursors.len() == 1 {
            let t = format!("\n{}", ind[0]);
            self.edit_each("newline", &move |_| t.clone());
        } else {
            self.edit_each("newline", &|_| "\n".to_string());
        }
    }

    pub fn backspace(&mut self) {
        if self.cursors.iter().all(|c| !c.has_sel()) {
            // turn "delete one char before" into a selection, then replace it with nothing
            for c in self.cursors.iter_mut() { if c.head > 0 { c.anchor = c.head - 1; } }
        }
        self.edit_each("del", &|_| String::new());
    }

    pub fn delete(&mut self) {
        let n = self.len();
        if self.cursors.iter().all(|c| !c.has_sel()) {
            for c in self.cursors.iter_mut() { if c.head < n { c.anchor = c.head + 1; } }
        }
        self.edit_each("del", &|_| String::new());
    }

    pub fn delete_word_back(&mut self) {
        if self.cursors.iter().all(|c| !c.has_sel()) {
            let targets: Vec<usize> = self.cursors.iter().map(|c| self.word_left(c.head)).collect();
            for (c, t) in self.cursors.iter_mut().zip(targets) { c.anchor = t; }
        }
        self.edit_each("del", &|_| String::new());
    }

    pub fn indent(&mut self, back: bool) {
        self.checkpoint("indent");
        self.normalize();
        let mut lines: Vec<usize> = vec![];
        for c in &self.cursors {
            let (a, b) = (self.text.char_to_line(c.start()), self.text.char_to_line(c.end()));
            for l in a..=b { if !lines.contains(&l) { lines.push(l); } }
        }
        if !back && self.cursors.iter().all(|c| !c.has_sel()) {
            // plain Tab with no selection: just insert 4 spaces at each caret
            self.last_kind = "";
            self.undo.pop();
            self.insert(&" ".repeat(crate::config::tab_width()));
            return;
        }
        lines.sort();
        let first = lines.first().copied().unwrap_or(0);
        for &l in lines.iter().rev() {
            let st = self.text.line_to_char(l);
            if back {
                let s = self.line_string(l);
                let n = if s.starts_with('\t') { 1 } else { s.chars().take(crate::config::tab_width()).take_while(|c| *c == ' ').count() };
                if n > 0 {
                    self.text.remove(st..st + n);
                    self.shift_after(st, -(n as isize));
                }
            } else {
                let w = crate::config::tab_width();
                self.text.insert(st, &" ".repeat(w));
                self.shift_after(st, w as isize);
            }
        }
        self.touched(first);
    }

    /// move every cursor end at or after `at` by delta (used by line operations that edit outside the cursors)
    fn shift_after(&mut self, at: usize, delta: isize) {
        for c in self.cursors.iter_mut() {
            if c.head >= at { c.head = (c.head as isize + delta).max(at as isize) as usize; }
            if c.anchor >= at { c.anchor = (c.anchor as isize + delta).max(at as isize) as usize; }
        }
    }

    pub fn delete_lines(&mut self) {
        self.checkpoint("line");
        self.normalize();
        let mut ranges: Vec<(usize, usize)> = vec![];
        for c in &self.cursors {
            let (a, b) = (self.text.char_to_line(c.start()), self.text.char_to_line(c.end()));
            match ranges.last_mut() { Some(r) if a <= r.1 + 1 && a >= r.0 => r.1 = r.1.max(b), _ => ranges.push((a, b)) }
        }
        let mut first = usize::MAX;
        for (a, b) in ranges.into_iter().rev() {
            let s = self.text.line_to_char(a);
            let e = if b + 1 < self.lines() { self.text.line_to_char(b + 1) } else { self.len() };
            self.text.remove(s..e);
            first = first.min(a);
            self.shift_after(e, -((e - s) as isize));
            for c in self.cursors.iter_mut() { if c.head > s && c.head < e { c.head = s; } if c.anchor > s && c.anchor < e { c.anchor = s; } }
        }
        for c in self.cursors.iter_mut() { c.anchor = c.head; }
        self.touched(first);
    }

    pub fn duplicate_lines(&mut self) {
        self.checkpoint("line");
        self.normalize();
        let mut first = usize::MAX;
        let mut ls: Vec<(usize, usize)> = self.cursors.iter().map(|c| (self.text.char_to_line(c.start()), self.text.char_to_line(c.end()))).collect();
        ls.dedup();
        for (a, b) in ls.into_iter().rev() {
            let s = self.text.line_to_char(a);
            let e = if b + 1 < self.lines() { self.text.line_to_char(b + 1) } else { self.len() };
            let mut chunk = self.text.slice(s..e).to_string();
            if !chunk.ends_with('\n') { chunk.insert(0, '\n'); }
            self.text.insert(e, &chunk);
            first = first.min(a);
            let n = chunk.chars().count();
            for c in self.cursors.iter_mut() {
                if c.start() >= e { c.head += n; c.anchor += n; }
            }
        }
        self.touched(first);
    }

    // ---------------------------------------------------------------- selection text
    pub fn selected_text(&self) -> String {
        let mut cs = self.cursors.clone();
        cs.sort_by_key(|c| c.start());
        let parts: Vec<String> = cs.iter().filter(|c| c.has_sel()).map(|c| self.text.slice(c.start()..c.end()).to_string()).collect();
        if parts.is_empty() {
            // micro: copy with no selection copies the line
            let l = self.text.char_to_line(self.primary().head);
            return format!("{}\n", self.line_string(l));
        }
        parts.join("\n")
    }

    pub fn cut(&mut self) -> String {
        if self.cursors.iter().all(|c| !c.has_sel()) {
            let t = self.selected_text();
            self.delete_lines();
            return t;
        }
        let t = self.selected_text();
        self.edit_each("cut", &|_| String::new());
        t
    }

    // ---------------------------------------------------------------- movement
    fn word_left(&self, mut p: usize) -> usize {
        while p > 0 && !is_word(self.text.char(p - 1)) { p -= 1; }
        while p > 0 && is_word(self.text.char(p - 1)) { p -= 1; }
        p
    }
    fn word_right(&self, mut p: usize) -> usize {
        let n = self.len();
        while p < n && !is_word(self.text.char(p)) { p += 1; }
        while p < n && is_word(self.text.char(p)) { p += 1; }
        p
    }
    pub fn word_at(&self, p: usize) -> (usize, usize) {
        let n = self.len();
        let p = p.min(n);
        let (mut a, mut b) = (p, p);
        while a > 0 && is_word(self.text.char(a - 1)) { a -= 1; }
        while b < n && is_word(self.text.char(b)) { b += 1; }
        (a, b)
    }

    pub fn move_cursors(&mut self, m: Move, select: bool) {
        self.break_undo();
        let snapshot = self.cursors.clone();
        for (i, c) in snapshot.iter().enumerate() {
            let (l, col) = self.pos(c.head);
            let mut want = c.want;
            let head = match m {
                Move::Left => if c.has_sel() && !select { c.start() } else { c.head.saturating_sub(1) },
                Move::Right => if c.has_sel() && !select { c.end() } else { (c.head + 1).min(self.len()) },
                Move::WordLeft => self.word_left(c.head),
                Move::WordRight => self.word_right(c.head),
                Move::Home => {
                    // smart home: first non-blank, then column 0
                    let s = self.line_string(l);
                    let ind = s.chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
                    self.idx(l, if col == ind { 0 } else { ind })
                }
                Move::End => self.idx(l, self.line_len(l)),
                Move::DocStart => 0,
                Move::DocEnd => { self.ensure_loaded(); self.len() }
                Move::Up | Move::Down | Move::PageUp(_) | Move::PageDown(_) => {
                    let w = want.unwrap_or(col);
                    want = Some(w);
                    let nl = match m {
                        Move::Up => l.saturating_sub(1),
                        Move::Down => (l + 1).min(self.lines() - 1),
                        Move::PageUp(n) => l.saturating_sub(n),
                        Move::PageDown(n) => (l + n).min(self.lines() - 1),
                        _ => l,
                    };
                    if matches!(m, Move::Up) && l == 0 { 0 } else if matches!(m, Move::Down) && l + 1 >= self.lines() { self.len() } else { self.idx(nl, w) }
                }
            };
            let keep_want = matches!(m, Move::Up | Move::Down | Move::PageUp(_) | Move::PageDown(_));
            self.cursors[i] = Cursor { head, anchor: if select { c.anchor } else { head }, want: if keep_want { want } else { None } };
        }
        self.normalize();
    }

    pub fn select_all(&mut self) { self.cursors = vec![Cursor { head: self.len(), anchor: 0, want: None }]; }
    pub fn collapse(&mut self) { let p = self.primary(); self.cursors = vec![Cursor::at(p.head)]; }
    pub fn click(&mut self, p: usize, extend: bool, add: bool) {
        self.break_undo();
        if add { self.cursors.push(Cursor::at(p)); }
        else if extend { let a = self.primary().anchor; self.cursors = vec![Cursor { head: p, anchor: a, want: None }]; }
        else { self.cursors = vec![Cursor::at(p)]; }
        self.normalize();
    }
    pub fn drag_to(&mut self, p: usize) {
        if let Some(c) = self.cursors.last_mut() { c.head = p; c.want = None; }
    }
    pub fn select_word(&mut self, p: usize) { let (a, b) = self.word_at(p); self.cursors = vec![Cursor { head: b, anchor: a, want: None }]; }
    pub fn select_line(&mut self, line: usize) {
        let s = self.idx(line, 0);
        let e = if line + 1 < self.lines() { self.text.line_to_char(line + 1) } else { self.len() };
        self.cursors = vec![Cursor { head: e, anchor: s, want: None }];
    }

    /// Ctrl-D: with no selection select the word under the caret, else add a cursor on the next occurrence of the selected text.
    pub fn add_next_occurrence(&mut self) -> bool {
        let last = self.primary();
        if !last.has_sel() {
            let (a, b) = self.word_at(last.head);
            if a == b { return false; }
            *self.cursors.last_mut().unwrap() = Cursor { head: b, anchor: a, want: None };
            return true;
        }
        let needle: Vec<char> = self.text.slice(last.start()..last.end()).chars().collect();
        let total = self.len();
        let hay: Vec<char> = self.text.chars().collect();
        let n = needle.len();
        let mut p = last.end();
        for _ in 0..total.max(1) {
            if p + n > total { p = 0; }
            if hay[p..p + n] == needle[..] && !self.cursors.iter().any(|c| c.start() == p && c.end() == p + n) {
                self.cursors.push(Cursor { head: p + n, anchor: p, want: None });
                return true;
            }
            p += 1;
        }
        false
    }

    /// Alt-Up / Alt-Down: a new cursor on the line above or below the primary one, in the same column.
    pub fn add_cursor_vertical(&mut self, down: bool) {
        let c = self.primary();
        let (l, col) = self.pos(c.head);
        let col = c.want.unwrap_or(col);
        let nl = if down { l + 1 } else { l.wrapping_sub(1) };
        if nl >= self.lines() { return; }
        let p = self.idx(nl, col);
        self.cursors.push(Cursor { head: p, anchor: p, want: Some(col) });
        self.normalize();
    }

    /// sort by position, merge cursors that overlap or sit on the same spot; the primary (last) cursor stays last
    pub fn normalize(&mut self) {
        let primary = self.primary();
        let mut cs = self.cursors.clone();
        cs.sort_by_key(|c| (c.start(), c.end()));
        let mut out: Vec<Cursor> = vec![];
        for c in cs {
            match out.last_mut() {
                Some(p) if c.start() < p.end() || (c.start() == p.start() && c.end() == p.end()) || (!c.has_sel() && !p.has_sel() && c.head == p.head) => {
                    if c.end() > p.end() { *p = Cursor { head: c.end(), anchor: p.start(), want: None }; }
                }
                _ => out.push(c),
            }
        }
        // put the primary cursor last
        if let Some(i) = out.iter().position(|c| c.start() <= primary.head && primary.head <= c.end()) {
            let c = out.remove(i);
            out.push(c);
        }
        if out.is_empty() { out.push(Cursor::at(0)); }
        self.cursors = out;
        let n = self.len();
        for c in self.cursors.iter_mut() { c.head = c.head.min(n); c.anchor = c.anchor.min(n); }
    }

    // ---------------------------------------------------------------- search
    /// case-insensitive search for `needle` inside single lines, wrapping around; line by line so a huge file is never copied
    pub fn find(&self, needle: &str, from: usize, forward: bool) -> Option<(usize, usize)> {
        if needle.is_empty() || needle.contains('\n') { return None; }
        let nd: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
        let n = nd.len();
        let total = self.lines();
        let from = from.min(self.len());
        let l0 = self.text.char_to_line(from);
        // hits in one line as char columns; None-length mismatch lines (rare lowercase expansions) are skipped
        let hits = |l: usize| -> Vec<usize> {
            let lc: Vec<char> = self.text.line(l).chars().flat_map(|c| c.to_lowercase()).collect();
            if lc.len() != self.text.line(l).len_chars() || lc.len() < n { return vec![]; }
            (0..=lc.len() - n).filter(|&p| lc[p..p + n] == nd[..]).collect()
        };
        for step in 0..=total {
            let l = if forward { (l0 + step) % total } else { (l0 + total - step % total) % total };
            let base = self.text.line_to_char(l);
            let hs = hits(l);
            let ok = |p: usize| if step == 0 { if forward { p >= from } else { p < from } } else if step == total { if forward { p < from } else { p >= from } } else { true };
            let mut it = hs.into_iter().map(|p| base + p).filter(|&p| ok(p));
            let pick = if forward { it.next() } else { it.last() };
            if let Some(p) = pick { return Some((p, p + n)); }
        }
        None
    }
    pub fn goto_line(&mut self, line: usize) {
        let p = self.idx(line.saturating_sub(1), 0);
        self.cursors = vec![Cursor::at(p)];
    }

    pub fn replace_all(&mut self, needle: &str, with: &str) -> usize {
        if needle.is_empty() { return 0; }
        let s = self.contents();
        let lower_same = s.to_lowercase().chars().count() == s.chars().count();
        if !lower_same { return 0; }
        let mut out = String::new();
        let (ls, ln) = (s.to_lowercase(), needle.to_lowercase());
        let sc: Vec<char> = s.chars().collect();
        let lc: Vec<char> = ls.chars().collect();
        let nc: Vec<char> = ln.chars().collect();
        let (mut i, mut count) = (0, 0);
        while i < sc.len() {
            if i + nc.len() <= lc.len() && lc[i..i + nc.len()] == nc[..] { out.push_str(with); i += nc.len(); count += 1; } else { out.push(sc[i]); i += 1; }
        }
        if count > 0 {
            self.checkpoint("replace");
            self.text = Rope::from_str(&out);
            self.cursors = vec![Cursor::at(0)];
            self.touched(0);
        }
        count
    }

    pub fn set_cursor_line_col(&mut self, line: usize, col: usize) {
        let p = self.idx(line, col);
        self.cursors = vec![Cursor::at(p)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Buffer { Buffer::from_text(s, None) }

    #[test]
    fn type_and_undo() {
        let mut x = b("");
        x.insert("a"); x.insert("b");
        assert_eq!(x.contents(), "ab");
        x.undo();
        assert_eq!(x.contents(), "");
        x.redo();
        assert_eq!(x.contents(), "ab");
    }

    #[test]
    fn multi_cursor_insert() {
        let mut x = b("a\nb\nc");
        x.cursors = vec![Cursor::at(0), Cursor::at(2), Cursor::at(4)];
        x.insert("-");
        assert_eq!(x.contents(), "-a\n-b\n-c");
        assert_eq!(x.cursors.iter().map(|c| c.head).collect::<Vec<_>>(), vec![1, 4, 7]);
    }

    #[test]
    fn next_occurrence() {
        let mut x = b("foo bar foo baz foo");
        x.cursors = vec![Cursor::at(1)];
        assert!(x.add_next_occurrence());      // selects "foo"
        assert!(x.add_next_occurrence());      // adds the second
        assert!(x.add_next_occurrence());      // adds the third
        assert!(!x.add_next_occurrence());     // all taken
        x.insert("X");
        assert_eq!(x.contents(), "X bar X baz X");
    }

    #[test]
    fn backspace_joins_and_multi() {
        let mut x = b("ab\ncd");
        x.cursors = vec![Cursor::at(3)];
        x.backspace();
        assert_eq!(x.contents(), "abcd");
        let mut y = b("xa\nxb");
        y.cursors = vec![Cursor::at(1), Cursor::at(4)];
        y.backspace();
        assert_eq!(y.contents(), "a\nb");
    }

    #[test]
    fn lines_and_clipboard() {
        let mut x = b("one\ntwo\nthree");
        x.cursors = vec![Cursor::at(5)];
        assert_eq!(x.cut(), "two\n");
        assert_eq!(x.contents(), "one\nthree");
        x.duplicate_lines();
        assert_eq!(x.contents(), "one\nthree\nthree");
    }

    #[test]
    fn find_wraps() {
        let x = b("Hello hello");
        assert_eq!(x.find("hello", 1, true), Some((6, 11)));
        assert_eq!(x.find("hello", 7, true), Some((0, 5)));
        assert_eq!(x.find("hello", 6, false), Some((0, 5)));
    }

    #[test]
    fn crlf_roundtrip() {
        let mut x = b("a\r\nb\r\n");
        assert!(x.crlf);
        assert_eq!(x.contents(), "a\nb\n");
        x.path = Some(std::env::temp_dir().join("frost-crlf-test.txt"));
        x.save().unwrap();
        assert_eq!(std::fs::read(x.path.as_ref().unwrap()).unwrap(), b"a\r\nb\r\n");
        let _ = std::fs::remove_file(x.path.as_ref().unwrap());
    }

    #[test]
    fn indent_selection() {
        let mut x = b("a\nb");
        x.select_all();
        x.indent(false);
        assert_eq!(x.contents(), "    a\n    b");
        x.indent(true);
        assert_eq!(x.contents(), "a\nb");
    }

    #[test]
    fn delete_lines_two_cursors() {
        let mut x = b("a\nb\nc\nd");
        x.cursors = vec![Cursor::at(0), Cursor::at(4)];
        x.delete_lines();
        assert_eq!(x.contents(), "b\nd");
        x.undo();
        assert_eq!(x.contents(), "a\nb\nc\nd");
    }

    #[test]
    fn delete_last_line_and_empty() {
        let mut x = b("only");
        x.delete_lines();
        assert_eq!(x.contents(), "");
        x.backspace(); x.delete(); x.delete_lines();
        assert_eq!(x.contents(), "");
        x.move_cursors(Move::Up, false); x.move_cursors(Move::Down, true); x.move_cursors(Move::PageDown(5), false);
    }

    #[test]
    fn paste_one_line_per_cursor_and_undo() {
        let mut x = b("1\n2");
        x.cursors = vec![Cursor::at(1), Cursor::at(3)];
        x.paste("a\nb");
        assert_eq!(x.contents(), "1a\n2b");
        x.undo();
        assert_eq!(x.contents(), "1\n2");
    }

    #[test]
    fn selection_replace_and_unicode() {
        let mut x = b("zażółć gęślą");
        x.cursors = vec![Cursor { head: 6, anchor: 0, want: None }];
        x.insert("X");
        assert_eq!(x.contents(), "X gęślą");
        x.cursors = vec![Cursor::at(x.len())];
        x.backspace();
        assert_eq!(x.contents(), "X gęśl");
    }

    #[test]
    fn duplicate_with_two_cursors_and_newline_indent() {
        let mut x = b("  a\nb");
        x.cursors = vec![Cursor::at(3)];
        x.newline();
        assert_eq!(x.contents(), "  a\n  \nb");
        let mut y = b("a\nb");
        y.cursors = vec![Cursor::at(0), Cursor::at(2)];
        y.duplicate_lines();
        assert_eq!(y.contents(), "a\na\nb\nb");
    }
}

// ------------------------------------------------------------------ lazy, streaming load of big files

/// files above this size open lazily: the first screen at once, the rest is read by all cores in the background
pub const LAZY_MIN: u64 = 8 << 20;
/// size of the part loaded before the first paint
const HEAD: usize = 2 << 20;
/// each worker reads its share in pieces of this size, so memory use is the text itself plus a little
const SUB: usize = 16 << 20;

pub struct Loader {
    rx: std::sync::mpsc::Receiver<(Rope, bool)>,
    done: std::sync::Arc<std::sync::atomic::AtomicU64>,
    total: u64,
}

#[cfg(unix)]
fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> std::io::Result<usize> { std::os::unix::fs::FileExt::read_at(f, buf, off) }
#[cfg(windows)]
fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> std::io::Result<usize> { std::os::windows::fs::FileExt::seek_read(f, buf, off) }

fn read_exact_at(f: &std::fs::File, mut buf: &mut [u8], mut off: u64) -> std::io::Result<()> {
    while !buf.is_empty() {
        let n = read_at(f, buf, off)?;
        if n == 0 { return Err(std::io::ErrorKind::UnexpectedEof.into()); }
        buf = &mut buf[n..]; off += n as u64;
    }
    Ok(())
}

/// first byte after the next '\n' at or after `from` (or the end of the file)
fn next_line_start(f: &std::fs::File, from: u64, size: u64) -> u64 {
    let mut pos = from;
    let mut blk = vec![0u8; 64 << 10];
    while pos < size {
        let n = ((size - pos) as usize).min(blk.len());
        if read_exact_at(f, &mut blk[..n], pos).is_err() { return size; }
        if let Some(i) = blk[..n].iter().position(|&c| c == b'\n') { return pos + i as u64 + 1; }
        pos += n as u64;
    }
    size
}

fn push_piece(rope: &mut Rope, crlf: &mut bool, bytes: &[u8]) {
    let s = String::from_utf8_lossy(bytes);
    if s.contains("\r\n") { *crlf = true; rope.append(Rope::from_str(&s.replace("\r\n", "\n"))); } else { rope.append(Rope::from_str(&s)); }
}

/// bytes [a, b) (both on line starts) as a rope
fn load_range(f: &std::fs::File, a: u64, b: u64, sub: usize, done: &std::sync::atomic::AtomicU64) -> (Rope, bool) {
    let mut rope = Rope::new();
    let mut crlf = false;
    let mut carry: Vec<u8> = vec![];
    let mut pos = a;
    while pos < b || !carry.is_empty() {
        let want = ((b - pos) as usize).min(sub);
        let mut buf = std::mem::take(&mut carry);
        let old = buf.len();
        buf.resize(old + want, 0);
        if read_exact_at(f, &mut buf[old..], pos).is_err() { break; }
        pos += want as u64;
        let cut = if pos >= b { buf.len() } else {
            match buf.iter().rposition(|&c| c == b'\n') { Some(i) => i + 1, None => { carry = buf; continue; } }
        };
        carry = buf.split_off(cut);
        push_piece(&mut rope, &mut crlf, &buf);
        done.fetch_add(buf.len() as u64, std::sync::atomic::Ordering::Relaxed);
    }
    (rope, crlf)
}

impl Buffer {
    /// Open a file. Small ones are read whole; big ones (>= `lazy_min`) show their first piece immediately while
    /// the rest streams in on all cores. Memory use is about the size of the text, not twice.
    pub fn open_with(path: &std::path::Path, lazy_min: u64, head: usize, sub: usize) -> std::io::Result<Self> {
        let f = std::fs::File::open(path)?;
        let size = f.metadata()?.len();
        let mut b = Buffer::new();
        b.path = Some(path.to_path_buf());
        if size < lazy_min {
            let mut bytes = vec![0u8; size as usize];
            read_exact_at(&f, &mut bytes, 0)?;
            let mut crlf = false;
            push_piece(&mut b.text, &mut crlf, &bytes);
            b.crlf = crlf;
            return Ok(b);
        }
        let cut = next_line_start(&f, head as u64, size);
        let done = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (h, c) = load_range(&f, 0, cut, sub, &done);
        b.text = h; b.crlf = c;
        let (tx, rx) = std::sync::mpsc::channel();
        let done2 = done.clone();
        std::thread::spawn(move || {
            let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
            let span = size - cut;
            let mut cuts = vec![cut];
            for k in 1..n {
                let nominal = cut + span * k as u64 / n as u64;
                let last = *cuts.last().unwrap();
                cuts.push(if nominal <= last { last } else { next_line_start(&f, nominal - 1, size).max(last) });
            }
            cuts.push(size);
            let parts: Vec<(Rope, bool)> = std::thread::scope(|sc| {
                let hs: Vec<_> = cuts.windows(2).map(|w| { let (f, d) = (&f, &done2); let (a, e) = (w[0], w[1]); sc.spawn(move || load_range(f, a, e, sub, d)) }).collect();
                hs.into_iter().map(|h| h.join().unwrap_or((Rope::new(), false))).collect()
            });
            let mut rope = Rope::new();
            let mut crlf = false;
            for (r, c) in parts { rope.append(r); crlf |= c; }
            let _ = tx.send((rope, crlf));
        });
        b.loading = Some(Loader { rx, done, total: size - cut });
        Ok(b)
    }

    pub fn open(path: &std::path::Path) -> std::io::Result<Self> { Buffer::open_with(path, LAZY_MIN, HEAD, SUB) }

    /// 0..=100 while the rest of a big file is still streaming in
    pub fn load_pct(&self) -> Option<u8> {
        self.loading.as_ref().map(|l| (l.done.load(std::sync::atomic::Ordering::Relaxed) * 100 / l.total.max(1)).min(99) as u8)
    }

    /// take the finished tail if it is ready; true when something changed
    pub fn poll_load(&mut self) -> bool {
        let got = match &self.loading { Some(l) => l.rx.try_recv().ok(), None => None };
        match got { Some(t) => { self.attach_tail(t); true } None => false }
    }

    /// wait for the rest of the file (before any edit, save, or jump to the end)
    pub fn ensure_loaded(&mut self) {
        let got = match &self.loading { Some(l) => l.rx.recv().ok(), None => return };
        match got { Some(t) => self.attach_tail(t), None => self.loading = None }
    }

    fn attach_tail(&mut self, (tail, crlf): (Rope, bool)) {
        self.text.append(tail);
        self.crlf |= crlf;
        self.loading = None;
        self.version += 1;
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    /// every case was produced by the Lean model in lean4/Frost.lean (whose `apply_delta` theorem is proved)
    #[test]
    fn delta_matches_the_lean_model_on_every_case() {
        let cases = include_str!("../tests/delta_cases.tsv");
        let mut n = 0;
        for line in cases.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 5, "bad line {line:?}");
            let d = delta(f[0], f[1]);
            assert_eq!((d.start, d.del, d.ins.as_str()), (f[2].parse().unwrap(), f[3].parse().unwrap(), f[4]), "{:?} -> {:?}", f[0], f[1]);
            n += 1;
        }
        assert_eq!(n, 121 * 121);
    }

    #[test]
    fn delta_roundtrip_with_unicode() {
        for (a, b) in [("abc", "abXc"), ("żółw", "żołw"), ("", "x"), ("x", ""), ("same", "same"), ("aXXb", "ab")] {
            let d = delta(a, b);
            let mut r = Rope::from_str(a);
            apply(&mut r, &d);
            assert_eq!(r.to_string(), b, "{a:?} -> {b:?}");
        }
    }
    #[test]
    fn history_survives_a_save_and_restore() {
        let mut x = Buffer::from_text("", None);
        for w in ["one ", "two ", "three"] { x.insert(w); x.break_undo(); }
        x.undo();
        let h = x.history().unwrap();
        let json = serde_json::to_string(&h).unwrap();
        let h2: History = serde_json::from_str(&json).unwrap();
        let mut y = Buffer::from_text(&x.contents(), None);
        assert!(y.set_history(&h2));
        assert_eq!(y.contents(), "one two ");
        y.redo();
        assert_eq!(y.contents(), "one two three");
        y.undo(); y.undo(); y.undo(); y.undo();
        assert_eq!(y.contents(), "");
        y.undo();
        assert_eq!(y.contents(), "");
    }
    #[test]
    fn history_for_other_text_is_refused() {
        let mut x = Buffer::from_text("", None);
        x.insert("abc");
        let h = x.history().unwrap();
        let mut y = Buffer::from_text("different", None);
        assert!(!y.set_history(&h));
        assert_eq!(y.contents(), "different");
    }
}

#[cfg(test)]
mod big_tests {
    use super::*;
    #[test]
    fn lazy_load_matches_plain() {
        let dir = std::env::temp_dir().join(format!("frost-lazy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("big.txt");
        let s: String = (0..30_000).map(|i| if i % 3 == 0 { format!("line {} żółw\r\n", i) } else { format!("x{}\n", i) }).collect();
        std::fs::write(&p, &s).unwrap();
        // tiny thresholds so the lazy path, several workers and many sub-pieces are all used
        let mut b = Buffer::open_with(&p, 1000, 5000, 4096).unwrap();
        assert!(b.loading.is_some());
        b.ensure_loaded();
        assert!(b.loading.is_none());
        assert!(b.crlf);
        assert_eq!(b.text.to_string(), s.replace("\r\n", "\n"));
        let plain = Buffer::open_with(&p, u64::MAX, 0, 0).unwrap();
        assert_eq!(plain.text.to_string(), b.text.to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
