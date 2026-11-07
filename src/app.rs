//! Editor state and input: tabs, menus, pickers, prompts, keyboard (CUA + micro) and mouse.
use crate::buffer::{Buffer, Move};
use crate::highlight::Highlighter;
use crate::session::{self, Session, TabState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    New, Open, Save, SaveAs, Close, NextTab, PrevTab, Quit,
    Undo, Redo, Cut, Copy, Paste, SelectAll, DuplicateLine, DeleteLine, Indent, Unindent,
    Find, FindNext, FindPrev, Replace, Goto, NextOccurrence, CursorUp, CursorDown, OneCursor,
    Language, Define, DiffDisk, DiffTab, Palette, ReopenClosed, ClosedTabs, SaveSession, LoadSession, ToggleLineNumbers, Help,
}

pub const MENUS: &[(&str, &[(&str, &str, Action)])] = &[
    ("File", &[("New", "Ctrl-N", Action::New), ("Open...", "Ctrl-O", Action::Open), ("Save", "Ctrl-S", Action::Save), ("Save As...", "Ctrl-Shift-S", Action::SaveAs),
               ("Close tab", "Ctrl-W", Action::Close), ("Diff with file on disk", "", Action::DiffDisk), ("Diff with another tab...", "", Action::DiffTab), ("Reopen closed tab", "Ctrl-T", Action::ReopenClosed), ("Closed tabs...", "", Action::ClosedTabs), ("Save session as...", "", Action::SaveSession), ("Load session...", "", Action::LoadSession), ("Quit", "Ctrl-Q", Action::Quit)]),
    ("Edit", &[("Undo", "Ctrl-Z", Action::Undo), ("Redo", "Ctrl-Y", Action::Redo), ("Cut", "Ctrl-X", Action::Cut), ("Copy", "Ctrl-C", Action::Copy), ("Paste", "Ctrl-V", Action::Paste),
               ("Select all", "Ctrl-A", Action::SelectAll), ("Duplicate line", "Ctrl-Shift-D", Action::DuplicateLine), ("Delete line", "Ctrl-K", Action::DeleteLine),
               ("Define word", "F2", Action::Define), ("Indent", "Tab", Action::Indent), ("Unindent", "Shift-Tab", Action::Unindent)]),
    ("Search", &[("Find...", "Ctrl-F", Action::Find), ("Find next", "F3", Action::FindNext), ("Find previous", "Shift-F3", Action::FindPrev), ("Replace all...", "Ctrl-H", Action::Replace),
                 ("Go to line...", "Ctrl-G", Action::Goto)]),
    ("Cursors", &[("Add next occurrence", "Ctrl-D", Action::NextOccurrence), ("Add cursor above", "Alt-Up", Action::CursorUp), ("Add cursor below", "Alt-Down", Action::CursorDown),
                  ("Single cursor", "Esc", Action::OneCursor)]),
    ("View", &[("Next tab", "Ctrl-PgDn", Action::NextTab), ("Previous tab", "Ctrl-PgUp", Action::PrevTab), ("Line numbers", "", Action::ToggleLineNumbers), ("Command palette", "Ctrl-P", Action::Palette)]),
    ("Language", &[("Choose language...", "Ctrl-L", Action::Language)]),
    ("Help", &[("Keys", "F1", Action::Help)]),
];

const PALETTE: &[(&str, Action)] = &[
    ("New file", Action::New), ("Open file", Action::Open), ("Save", Action::Save), ("Save as", Action::SaveAs), ("Close tab", Action::Close), ("Quit", Action::Quit),
    ("Undo", Action::Undo), ("Redo", Action::Redo), ("Select all", Action::SelectAll), ("Duplicate line", Action::DuplicateLine), ("Delete line", Action::DeleteLine),
    ("Find", Action::Find), ("Replace all", Action::Replace), ("Go to line", Action::Goto), ("Add next occurrence (multi-cursor)", Action::NextOccurrence),
    ("Add cursor above", Action::CursorUp), ("Add cursor below", Action::CursorDown), ("Single cursor", Action::OneCursor), ("Choose language", Action::Language),
    ("Define word", Action::Define), ("Diff with file on disk", Action::DiffDisk), ("Diff with another tab", Action::DiffTab), ("Reopen closed tab", Action::ReopenClosed), ("Closed tabs", Action::ClosedTabs), ("Save session as", Action::SaveSession), ("Load session", Action::LoadSession), ("Toggle line numbers", Action::ToggleLineNumbers), ("Keys", Action::Help),
];

pub const HELP: &str = "Ctrl-S save  Ctrl-O open  Ctrl-N new  Ctrl-W close  Ctrl-Q quit | Ctrl-Z/Y undo/redo  Ctrl-X/C/V  Ctrl-A all  Ctrl-K del line | \
Ctrl-F find  F3/Shift-F3  Ctrl-H replace  Ctrl-G line | Ctrl-D next occurrence  Alt-Up/Down more cursors  Alt-click add cursor  Esc one cursor | \
Ctrl-PgUp/PgDn tabs  Ctrl-L language  Ctrl-P palette  mouse: click, drag, double-click word, triple-click line, wheel";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PromptKind { Open, SaveAs, Find, Goto, ReplaceFind, ReplaceWith, SaveSession }

pub struct Prompt { pub kind: PromptKind, pub label: String, pub input: String, pub aux: String }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerKind { Palette, Language, Sessions, Closed, DiffTab }

pub struct Picker { pub kind: PickerKind, pub filter: String, pub items: Vec<String>, pub sel: usize }

impl Picker {
    pub fn shown(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        let mut v: Vec<usize> = (0..self.items.len()).filter(|&i| f.split_whitespace().all(|w| self.items[i].to_lowercase().contains(w))).collect();
        // names that start with the typed text come first, the rest keep their (alphabetical) order
        if !f.is_empty() && self.kind == PickerKind::Language { v.sort_by_key(|&i| !self.items[i].to_lowercase().starts_with(&f)); }
        v
    }
}

#[derive(Default)]
pub struct Layout {
    pub text: Rect,
    pub gutter: u16,
    pub tabs: Vec<(Rect, usize)>,
    pub menus: Vec<(Rect, usize)>,
    pub dropdown: Option<(Rect, usize)>, // area and menu index
    pub picker: Option<Rect>,
    pub caret: Option<(u16, u16)>,
}

pub struct App {
    pub tabs: Vec<Buffer>,
    pub active: usize,
    pub hl: Highlighter,
    pub prompt: Option<Prompt>,
    pub picker: Option<Picker>,
    pub menu: Option<usize>,
    pub msg: String,
    pub quit: bool,
    pub line_numbers: bool,
    pub layout: Layout,
    pub show_help: bool,
    pub last_find: String,
    clip: String,
    dragging: bool,
    clicks: (Instant, u16, u16, u8),
    confirm_quit: bool,
    pub session_dir: PathBuf,
    pub hl_key: (u64, u64),
    pub closed: Vec<TabState>,
    pub hover_row: Option<usize>,
    pub keymap: std::collections::HashMap<String, Action>,
    pub dict: crate::dict::Dict,
    /// word suggestions under the caret: (word, meaning) list, selected row, chars already typed
    pub suggest: Option<(Vec<(String, String)>, usize, usize)>,
}

/// (program, args) pairs tried in order: Wayland, X11, macOS, Windows
fn clip_set_cmds() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("wl-copy", vec![]), ("xclip", vec!["-selection", "clipboard"]), ("xsel", vec!["-ib"]), ("pbcopy", vec![]), ("clip.exe", vec![]), ("clip", vec![])]
}
fn clip_get_cmds() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("wl-paste", vec!["-n"]), ("xclip", vec!["-selection", "clipboard", "-o"]), ("xsel", vec!["-ob"]), ("pbpaste", vec![]),
         ("powershell.exe", vec!["-NoProfile", "-Command", "Get-Clipboard -Raw"])]
}

fn set_system_clipboard(s: &str) {
    for cmd in clip_set_cmds() {
        if let Ok(mut c) = Command::new(cmd.0).args(&cmd.1).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            if let Some(mut i) = c.stdin.take() { let _ = i.write_all(s.as_bytes()); }
            let _ = c.wait();
            return;
        }
    }
}

fn get_system_clipboard() -> Option<String> {
    for cmd in clip_get_cmds() {
        if let Ok(o) = Command::new(cmd.0).args(&cmd.1).stderr(Stdio::null()).output() {
            if o.status.success() { return Some(String::from_utf8_lossy(&o.stdout).into_owned()); }
        }
    }
    None
}

impl App {
    pub fn new(files: &[String]) -> App {
        let hl = Highlighter::new();
        let mut app = App { tabs: vec![], active: 0, hl, prompt: None, picker: None, menu: None, msg: String::new(), quit: false, line_numbers: true,
                            layout: Layout::default(), show_help: false, last_find: String::new(), clip: String::new(), dragging: false,
                            clicks: (Instant::now(), 0, 0, 0), confirm_quit: false, session_dir: session::session_dir(), hl_key: (0, 0), closed: vec![], hover_row: None, keymap: Default::default(), dict: crate::dict::Dict::load(None), suggest: None };
        let cfg = crate::config::load();
        app.line_numbers = cfg.line_numbers;
        app.keymap = cfg.keys.iter().filter_map(|(k, v)| action_by_name(v).map(|a| (k.clone(), a))).collect();
        if cfg.dictionary.is_some() { app.dict = crate::dict::Dict::load(cfg.dictionary.clone()); }
        if let Some(e) = cfg.error { app.msg = e; }
        // like Notepad++: the previous session always comes back, files named on the command line are added to it
        app.restore(&session::load());
        for f in files { app.open_path(PathBuf::from(f)); }
        if app.tabs.is_empty() { app.new_tab(); }
        app
    }

    pub fn buf(&mut self) -> &mut Buffer { let i = self.active.min(self.tabs.len() - 1); &mut self.tabs[i] }

    fn new_tab(&mut self) {
        self.tabs.push(Buffer::new());
        self.active = self.tabs.len() - 1;
    }

    pub fn open_path(&mut self, p: PathBuf) {
        if let Some(i) = self.tabs.iter().position(|b| b.path.as_ref() == Some(&p)) { self.active = i; return; }
        let mut b = if p.exists() {
            match Buffer::open(&p) { Ok(b) => b, Err(e) => { self.msg = format!("cannot open {}: {e}", p.display()); return; } }
        } else {
            let mut b = Buffer::new();
            b.path = Some(p.clone());
            b
        };
        let first = b.text.line(0).to_string();
        b.syntax = self.hl.detect(p.to_str(), &first);
        // replace a lone empty untitled tab
        if self.tabs.len() == 1 && self.tabs[0].path.is_none() && !self.tabs[0].modified && self.tabs[0].len() == 0 { self.tabs.clear(); }
        self.tabs.push(b);
        self.active = self.tabs.len() - 1;
    }

    // ---------------------------------------------------------------- sessions
    pub fn snapshot(&mut self, with_history: bool) -> Session {
        let hl = &self.hl;
        Session {
            active: self.active.min(self.tabs.iter().filter(|b| b.title.is_none()).count().saturating_sub(1)),
            tabs: self.tabs.iter().filter(|b| b.title.is_none()).map(|b| Self::tab_state(hl, b, with_history)).collect(),
            closed: self.closed.clone(),
        }
    }

    fn tab_state(hl: &Highlighter, b: &Buffer, with_history: bool) -> TabState {
        let (l, c) = b.pos(b.primary().head);
        TabState { path: b.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                   unsaved: if b.modified || b.path.is_none() { Some(b.contents()) } else { None },
                   cursor_line: l, cursor_col: c, scroll: b.scroll, syntax: Some(hl.name_of(b.syntax)),
                   history: if with_history { b.history() } else { None } }
    }

    fn tab_from_state(&mut self, t: &TabState) -> Option<Buffer> {
        let mut b = match (&t.unsaved, &t.path) {
            (Some(text), p) => { let mut b = Buffer::from_text(text, p.as_ref().map(PathBuf::from)); b.modified = true; b }
            (None, Some(p)) => Buffer::open(std::path::Path::new(p)).ok()?,
            (None, None) => return None,
        };
        let first = b.text.line(0).to_string();
        b.syntax = t.syntax.as_deref().and_then(|n| self.hl.index_of(n)).unwrap_or_else(|| self.hl.detect(t.path.as_deref(), &first));
        b.set_cursor_line_col(t.cursor_line, t.cursor_col);
        b.scroll = t.scroll;
        if let Some(h) = &t.history { b.set_history(h); }
        Some(b)
    }

    /// remember a tab that is being closed (not an empty untitled one)
    fn remember_closed(&mut self, i: usize) {
        let b = &self.tabs[i];
        if b.path.is_none() && b.len() == 0 { return; }
        let st = Self::tab_state(&self.hl, b, true);
        self.closed.retain(|c| !(c.path.is_some() && c.path == st.path));
        self.closed.push(st);
        if self.closed.len() > 30 { self.closed.remove(0); }
    }

    pub fn restore(&mut self, s: &Session) {
        self.closed = s.closed.clone();
        for t in &s.tabs {
            if let Some(b) = self.tab_from_state(t) { self.tabs.push(b); }
        }
        self.active = s.active.min(self.tabs.len().saturating_sub(1));
    }

    /// called from the event loop: finish big-file loads; true while any is still running
    pub fn tick(&mut self) -> bool {
        let mut busy = false;
        for t in &mut self.tabs { t.poll_load(); busy |= t.loading.is_some(); }
        busy
    }

    /// `full` also writes the undo history (on exit); the 30 s autosave leaves it out because computing it costs time
    pub fn save_session(&mut self, full: bool) {
        let s = self.snapshot(full);
        if let Err(e) = session::save(&s) { eprintln!("frost: cannot save session: {e}"); }
    }

    // ---------------------------------------------------------------- clipboard
    fn copy_text(&mut self, t: String) { set_system_clipboard(&t); self.clip = t; }
    fn paste_text(&mut self) -> String {
        match get_system_clipboard() { Some(s) if !s.is_empty() => s, _ => self.clip.clone() }
    }

    // ---------------------------------------------------------------- actions
    pub fn run(&mut self, a: Action) {
        self.menu = None;
        self.msg.clear();
        match a {
            Action::New => self.new_tab(),
            Action::Open => self.prompt = Some(Prompt { kind: PromptKind::Open, label: "Open:".into(), input: String::new(), aux: String::new() }),
            Action::Save => self.save_active(),
            Action::SaveAs => {
                let cur = self.buf().path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                self.prompt = Some(Prompt { kind: PromptKind::SaveAs, label: "Save as:".into(), input: cur, aux: String::new() });
            }
            Action::Define => {
                let (w, _) = self.buf().word_at_caret();
                self.msg = if w.is_empty() { "put the caret on a word".into() } else {
                    match self.dict.define(&w) { Some(m) => format!("{w}: {m}"), None => format!("{w}: no entry (add it to ~/.config/frost/dictionary.txt)") }
                };
            }
            Action::DiffDisk => {
                let b = self.buf();
                let Some(p) = b.path.clone() else { self.msg = "save the file first - nothing on disk to compare with".into(); return; };
                if b.len() > 64 << 20 { self.msg = "diff is limited to files under 64 MB".into(); return; }
                let disk = match std::fs::read(&p) { Ok(x) => String::from_utf8_lossy(&x).replace("\r\n", "\n"), Err(e) => { self.msg = format!("cannot read {}: {e}", p.display()); return; } };
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let now = b.contents();
                self.show_diff(format!("diff: {name}"), &format!("{name} (disk)"), &disk, &format!("{name} (editor)"), &now);
            }
            Action::DiffTab => {
                if self.tabs.len() < 2 { self.msg = "open a second tab to compare with".into(); } else {
                    let items = self.tabs.iter().enumerate().filter(|(i, _)| *i != self.active).map(|(i, t)| format!("{}. {}", i + 1, t.name())).collect();
                    self.picker = Some(Picker { kind: PickerKind::DiffTab, filter: String::new(), items, sel: 0 });
                }
            }
            Action::Close => self.close_active(false),
            Action::ReopenClosed => match self.closed.pop() {
                Some(t) => match self.tab_from_state(&t) { Some(b) => { self.tabs.push(b); self.active = self.tabs.len() - 1; } None => self.msg = "that file is gone".into() },
                None => self.msg = "no closed tabs".into(),
            },
            Action::ClosedTabs => {
                if self.closed.is_empty() { self.msg = "no closed tabs".into(); } else {
                    let items = self.closed.iter().rev().enumerate().map(|(n, t)| {
                        let name = t.path.as_deref().map(|p| p.to_string()).unwrap_or_else(|| "untitled".into());
                        format!("{}. {}{}", n + 1, name, if t.unsaved.is_some() { "  (unsaved text)" } else { "" })
                    }).collect();
                    self.picker = Some(Picker { kind: PickerKind::Closed, filter: String::new(), items, sel: 0 });
                }
            }
            Action::NextTab => self.active = (self.active + 1) % self.tabs.len(),
            Action::PrevTab => self.active = (self.active + self.tabs.len() - 1) % self.tabs.len(),
            Action::Quit => {
                // the session keeps unsaved work, so quitting never loses anything - say so instead of nagging
                self.quit = true;
            }
            Action::Undo => self.buf().undo(),
            Action::Redo => self.buf().redo(),
            Action::Cut => { let t = self.buf().cut(); self.copy_text(t); }
            Action::Copy => { let t = self.buf().selected_text(); self.copy_text(t); }
            Action::Paste => { let t = self.paste_text(); self.buf().paste(&t); }
            Action::SelectAll => self.buf().select_all(),
            Action::DuplicateLine => self.buf().duplicate_lines(),
            Action::DeleteLine => self.buf().delete_lines(),
            Action::Indent => self.buf().indent(false),
            Action::Unindent => self.buf().indent(true),
            Action::Find => {
                let sel = { let b = self.buf(); if b.primary().has_sel() { b.selected_text() } else { String::new() } };
                let input = if sel.is_empty() || sel.contains('\n') { self.last_find.clone() } else { sel };
                self.prompt = Some(Prompt { kind: PromptKind::Find, label: "Find:".into(), input, aux: String::new() });
            }
            Action::FindNext => self.find_step(true),
            Action::FindPrev => self.find_step(false),
            Action::Replace => self.prompt = Some(Prompt { kind: PromptKind::ReplaceFind, label: "Replace all - find:".into(), input: self.last_find.clone(), aux: String::new() }),
            Action::Goto => self.prompt = Some(Prompt { kind: PromptKind::Goto, label: "Go to line:".into(), input: String::new(), aux: String::new() }),
            Action::NextOccurrence => { if !self.buf().add_next_occurrence() { self.msg = "no more occurrences".into(); } }
            Action::CursorUp => self.buf().add_cursor_vertical(false),
            Action::CursorDown => self.buf().add_cursor_vertical(true),
            Action::OneCursor => self.buf().collapse(),
            Action::Language => self.picker = Some(Picker { kind: PickerKind::Language, filter: String::new(), items: { let mut n = self.hl.language_names(); n.sort_by_key(|s| s.to_lowercase()); n.dedup(); n }, sel: 0 }),
            Action::Palette => self.picker = Some(Picker { kind: PickerKind::Palette, filter: String::new(), items: PALETTE.iter().map(|p| p.0.to_string()).collect(), sel: 0 }),
            Action::SaveSession => self.prompt = Some(Prompt { kind: PromptKind::SaveSession, label: "Save session as:".into(), input: String::new(), aux: String::new() }),
            Action::LoadSession => {
                let names = session::list_named(&self.session_dir);
                if names.is_empty() { self.msg = "no saved sessions".into(); } else { self.picker = Some(Picker { kind: PickerKind::Sessions, filter: String::new(), items: names, sel: 0 }); }
            }
            Action::ToggleLineNumbers => self.line_numbers = !self.line_numbers,
            Action::Help => self.show_help = !self.show_help,
        }
    }

    fn save_active(&mut self) {
        if self.buf().path.is_none() { self.run(Action::SaveAs); return; }
        match self.buf().save() { Ok(()) => { let n = self.buf().name(); self.msg = format!("saved {n}"); } Err(e) => self.msg = format!("save failed: {e}") }
    }

    fn close_active(&mut self, _force: bool) {
        self.remember_closed(self.active.min(self.tabs.len() - 1));
        if self.tabs.len() == 1 {
            // last tab: becomes an empty untitled one, like Notepad++
            self.tabs[0] = Buffer::new();
            return;
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
    }

    fn find_step(&mut self, fwd: bool) {
        if self.last_find.is_empty() { self.run(Action::Find); return; }
        let needle = self.last_find.clone();
        let b = self.buf();
        let c = b.primary();
        let from = if fwd { c.end() } else { c.start() };
        b.ensure_loaded();
        match b.find(&needle, from, fwd) {
            Some((s, e)) => b.cursors = vec![crate::buffer::Cursor { head: e, anchor: s, want: None }],
            None => self.msg = format!("not found: {needle}"),
        }
    }

    // ---------------------------------------------------------------- keyboard
    pub fn on_key(&mut self, k: KeyEvent) {
        if self.suggest.is_some() && self.picker.is_none() && self.prompt.is_none() && self.menu.is_none() {
            let n = self.suggest.as_ref().map(|s| s.0.len()).unwrap_or(0);
            match k.code {
                KeyCode::Tab if k.modifiers.is_empty() => { self.accept_suggestion(); return; }
                KeyCode::Down => { if let Some(s) = self.suggest.as_mut() { s.1 = (s.1 + 1) % n; } return; }
                KeyCode::Up => { if let Some(s) = self.suggest.as_mut() { s.1 = (s.1 + n - 1) % n; } return; }
                KeyCode::Esc => { self.suggest = None; return; }
                _ => {}
            }
        }
        let typing = matches!(k.code, KeyCode::Char(_) | KeyCode::Backspace) && !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        self.on_key_inner(k);
        if typing { self.update_suggest(); } else { self.suggest = None; }
    }

    fn show_diff(&mut self, title: String, an: &str, at: &str, bn: &str, bt: &str) {
        let d = crate::diff::unified(an, at, bn, bt);
        if d.is_empty() { self.msg = "no differences".into(); return; }
        let mut b = Buffer::from_text(&d, None);
        b.title = Some(title);
        b.syntax = self.hl.index_of("Diff").unwrap_or(0);
        self.tabs.push(b);
        self.active = self.tabs.len() - 1;
    }

    fn update_suggest(&mut self) {
        self.suggest = None;
        if self.picker.is_some() || self.prompt.is_some() || self.tabs.is_empty() { return; }
        let b = &self.tabs[self.active.min(self.tabs.len() - 1)];
        if b.cursors.len() != 1 || b.cursors[0].has_sel() { return; }
        let (w, start) = b.word_at_caret();
        let typed = b.cursors[0].head - start;
        if typed < 2 || typed != w.chars().count() { return; }   // only while the caret is at the end of the word
        let items = self.dict.complete(&w, &b.nearby_words(300), 8);
        if !items.is_empty() { self.suggest = Some((items, 0, typed)); }
    }

    fn accept_suggestion(&mut self) {
        if let Some((items, sel, n)) = self.suggest.take() { self.buf().replace_before_caret(n, &items[sel].0); }
    }

    fn on_key_inner(&mut self, k: KeyEvent) {
        self.confirm_quit = false;
        if self.picker.is_none() && self.prompt.is_none() && self.menu.is_none() && !self.keymap.is_empty() {
            if let Some(&a) = self.keymap.get(&key_spec(&k)) { return self.run(a); }
        }
        if self.show_help { self.show_help = false; if k.code == KeyCode::Esc { return; } }
        if self.picker.is_some() { return self.picker_key(k); }
        if self.prompt.is_some() { return self.prompt_key(k); }
        if let Some(m) = self.menu { return self.menu_key(k, m); }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let page = self.layout.text.height.max(2) as usize - 1;
        if ctrl {
            if let KeyCode::Char(c) = k.code {
                let a = match c.to_ascii_lowercase() {
                    's' if shift => Some(Action::SaveAs), 's' => Some(Action::Save), 'o' => Some(Action::Open), 'n' => Some(Action::New), 'w' => Some(Action::Close),
                    'q' => Some(Action::Quit), 'z' if shift => Some(Action::Redo), 'z' => Some(Action::Undo), 'y' => Some(Action::Redo),
                    'x' => Some(Action::Cut), 'c' => Some(Action::Copy), 'v' => Some(Action::Paste), 'a' => Some(Action::SelectAll),
                    'f' => Some(Action::Find), 'h' => Some(Action::Replace), 'g' => Some(Action::Goto), 'd' if shift => Some(Action::DuplicateLine),
                    'd' => Some(Action::NextOccurrence), 't' => Some(Action::ReopenClosed), 'k' => Some(Action::DeleteLine), 'l' => Some(Action::Language), 'p' | 'e' => Some(Action::Palette),
                    _ => None,
                };
                if let Some(a) = a { return self.run(a); }
            }
        }
        match k.code {
            KeyCode::Char(c) if !ctrl && !alt => self.buf().insert(&c.to_string()),
            KeyCode::Enter => self.buf().newline(),
            KeyCode::Backspace => if ctrl { self.buf().delete_word_back() } else { self.buf().backspace() },
            KeyCode::Delete => self.buf().delete(),
            KeyCode::Tab => self.run(Action::Indent),
            KeyCode::BackTab => self.run(Action::Unindent),
            KeyCode::Esc => { self.buf().collapse(); }
            KeyCode::Left => self.buf().move_cursors(if ctrl { Move::WordLeft } else { Move::Left }, shift),
            KeyCode::Right => self.buf().move_cursors(if ctrl { Move::WordRight } else { Move::Right }, shift),
            KeyCode::Up if alt => self.run(Action::CursorUp),
            KeyCode::Down if alt => self.run(Action::CursorDown),
            KeyCode::Up => self.buf().move_cursors(Move::Up, shift),
            KeyCode::Down => self.buf().move_cursors(Move::Down, shift),
            KeyCode::Home => self.buf().move_cursors(if ctrl { Move::DocStart } else { Move::Home }, shift),
            KeyCode::End => self.buf().move_cursors(if ctrl { Move::DocEnd } else { Move::End }, shift),
            KeyCode::PageUp if ctrl => self.run(Action::PrevTab),
            KeyCode::PageDown if ctrl => self.run(Action::NextTab),
            KeyCode::PageUp => self.buf().move_cursors(Move::PageUp(page), shift),
            KeyCode::PageDown => self.buf().move_cursors(Move::PageDown(page), shift),
            KeyCode::F(1) => self.run(Action::Help),
            KeyCode::F(2) => self.run(Action::Define),
            KeyCode::F(3) => self.run(if shift { Action::FindPrev } else { Action::FindNext }),
            KeyCode::F(10) => self.menu = Some(0),
            _ => {}
        }
    }

    fn menu_key(&mut self, k: KeyEvent, m: usize) {
        match k.code {
            KeyCode::Esc => self.menu = None,
            KeyCode::Left => self.menu = Some((m + MENUS.len() - 1) % MENUS.len()),
            KeyCode::Right => self.menu = Some((m + 1) % MENUS.len()),
            KeyCode::Char(c) => {
                // first letter of an item runs it
                if let Some(it) = MENUS[m].1.iter().find(|i| i.0.to_lowercase().starts_with(c.to_ascii_lowercase())) { self.run(it.2); }
            }
            _ => {}
        }
    }

    fn prompt_key(&mut self, k: KeyEvent) {
        let Some(p) = self.prompt.as_mut() else { return };
        match k.code {
            KeyCode::Esc => { self.prompt = None; }
            KeyCode::Backspace => { p.input.pop(); self.live_find(); }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => { p.input.push(c); self.live_find(); }
            KeyCode::Enter => { let p = self.prompt.take().unwrap(); self.finish_prompt(p); }
            KeyCode::F(3) => self.find_step(!k.modifiers.contains(KeyModifiers::SHIFT)),
            _ => {}
        }
    }

    /// incremental search while typing in the Find prompt
    fn live_find(&mut self) {
        let Some(p) = self.prompt.as_ref() else { return };
        if p.kind != PromptKind::Find { return; }
        let needle = p.input.clone();
        let b = self.buf();
        let from = b.primary().start();
        b.ensure_loaded();
        if let Some((s, e)) = b.find(&needle, from, true) { b.cursors = vec![crate::buffer::Cursor { head: e, anchor: s, want: None }]; }
    }

    fn finish_prompt(&mut self, p: Prompt) {
        match p.kind {
            PromptKind::Open => if !p.input.trim().is_empty() { self.open_path(PathBuf::from(shellexpand(p.input.trim()))); },
            PromptKind::SaveAs => if !p.input.trim().is_empty() {
                let path = PathBuf::from(shellexpand(p.input.trim()));
                let first = self.buf().text.line(0).to_string();
                let syn = self.hl.detect(path.to_str(), &first);
                let b = self.buf();
                b.path = Some(path);
                b.syntax = syn;
                self.save_active();
            },
            PromptKind::Find => { self.last_find = p.input; self.find_step(true); }
            PromptKind::Goto => if let Ok(n) = p.input.trim().parse::<usize>() { self.buf().goto_line(n); },
            PromptKind::ReplaceFind => {
                self.last_find = p.input.clone();
                self.prompt = Some(Prompt { kind: PromptKind::ReplaceWith, label: format!("Replace '{}' with:", p.input), input: String::new(), aux: p.input });
            }
            PromptKind::ReplaceWith => { let n = self.buf().replace_all(&p.aux, &p.input); self.msg = format!("{n} replaced"); }
            PromptKind::SaveSession => {
                let s = self.snapshot(true);
                self.msg = match session::save_named(&self.session_dir, p.input.trim(), &s) { Ok(()) => format!("session '{}' saved", p.input.trim()), Err(e) => format!("session: {e}") };
            }
        }
    }

    fn picker_key(&mut self, k: KeyEvent) {
        let Some(pk) = self.picker.as_mut() else { return };
        let n = pk.shown().len();
        match k.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Up => pk.sel = pk.sel.saturating_sub(1),
            KeyCode::Down => pk.sel = (pk.sel + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp => pk.sel = pk.sel.saturating_sub(10),
            KeyCode::PageDown => pk.sel = (pk.sel + 10).min(n.saturating_sub(1)),
            KeyCode::Backspace => { pk.filter.pop(); pk.sel = 0; }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => { pk.filter.push(c); pk.sel = 0; }
            KeyCode::Enter => {
                let pk = self.picker.take().unwrap();
                if let Some(&i) = pk.shown().get(pk.sel) { self.picked(pk.kind, &pk.items[i].clone()); }
            }
            _ => {}
        }
    }

    fn picked(&mut self, kind: PickerKind, item: &str) {
        match kind {
            PickerKind::Palette => if let Some(a) = PALETTE.iter().find(|p| p.0 == item) { self.run(a.1); },
            PickerKind::Language => if let Some(i) = self.hl.index_of(item) { self.buf().syntax = i; self.hl.invalidate_from(0); },
            PickerKind::DiffTab => {
                let n: usize = item.split('.').next().and_then(|x| x.parse().ok()).unwrap_or(0);
                if n >= 1 && n <= self.tabs.len() && n - 1 != self.active {
                    let (a, b) = (&self.tabs[n - 1], &self.tabs[self.active]);
                    if a.len() + b.len() > 64 << 20 { self.msg = "diff is limited to 64 MB".into(); } else {
                        let (an, bn, at, bt) = (a.name(), b.name(), a.contents(), b.contents());
                        self.show_diff(format!("diff: {an} / {bn}"), &an, &at, &bn, &bt);
                    }
                }
            }
            PickerKind::Closed => {
                let n: usize = item.split('.').next().and_then(|x| x.parse().ok()).unwrap_or(0);
                if n >= 1 && n <= self.closed.len() {
                    let t = self.closed.remove(self.closed.len() - n);
                    match self.tab_from_state(&t) { Some(b) => { self.tabs.push(b); self.active = self.tabs.len() - 1; } None => self.msg = "that file is gone".into() }
                }
            }
            PickerKind::Sessions => if let Some(s) = session::load_named(&self.session_dir, item) { self.tabs.clear(); self.restore(&s); if self.tabs.is_empty() { self.new_tab(); } },
        }
    }

    // ---------------------------------------------------------------- mouse
    fn hit(r: Rect, x: u16, y: u16) -> bool { x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height }

    /// buffer char index under a screen cell
    fn index_at(&mut self, x: u16, y: u16) -> usize {
        let t = self.layout.text;
        let g = self.layout.gutter;
        let b = self.buf();
        let line = (b.scroll + y.saturating_sub(t.y) as usize).min(b.lines() - 1);
        let want = (x.saturating_sub(t.x + g)) as usize + b.hscroll;
        let s = b.line_string(line);
        let (mut col, mut disp) = (0usize, 0usize);
        for ch in s.chars() {
            let w = if ch == '\t' { 4 - disp % 4 } else { 1 };
            if disp + w > want + (w / 2) && disp >= want || disp + w > want { break; }
            disp += w;
            col += 1;
        }
        b.idx(line, col)
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(pr) = self.layout.picker {
                    if !Self::hit(pr, x, y) { self.picker = None; }
                    return;
                }
                if let Some((r, mi)) = self.layout.dropdown {
                    if Self::hit(r, x, y) {
                        let row = (y - r.y) as usize;
                        if row >= 1 { if let Some(it) = MENUS[mi].1.get(row - 1) { self.run(it.2); } }
                        return;
                    }
                }
                if let Some((_, i)) = self.layout.menus.iter().find(|(r, _)| Self::hit(*r, x, y)).copied() {
                    self.menu = Some(i);
                    return;
                }
                self.menu = None;
                if let Some((_, i)) = self.layout.tabs.iter().find(|(r, _)| Self::hit(*r, x, y)).copied() { self.active = i.min(self.tabs.len() - 1); return; }
                if Self::hit(self.layout.text, x, y) {
                    let p = self.index_at(x, y);
                    let now = Instant::now();
                    let (t0, x0, y0, n0) = self.clicks;
                    let n = if now.duration_since(t0) < Duration::from_millis(450) && (x0, y0) == (x, y) { n0 % 3 + 1 } else { 1 };
                    self.clicks = (now, x, y, n);
                    let alt = m.modifiers.contains(KeyModifiers::ALT) || m.modifiers.contains(KeyModifiers::CONTROL);
                    let shift = m.modifiers.contains(KeyModifiers::SHIFT);
                    match n {
                        2 => self.buf().select_word(p),
                        3 => { let l = self.buf().pos(p).0; self.buf().select_line(l); }
                        _ => { self.buf().click(p, shift, alt); self.dragging = true; }
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                let t = self.layout.text;
                let (cx, cy) = (x.clamp(t.x, t.x + t.width.saturating_sub(1)), y.clamp(t.y, t.y + t.height.saturating_sub(1)));
                let p = self.index_at(cx, cy);
                self.buf().drag_to(p);
            }
            MouseEventKind::Up(_) => self.dragging = false,
            MouseEventKind::Moved => {
                // like Fresh: just pointing at a menu opens it, moving away from the bar and the open menu closes it
                if self.prompt.is_some() || self.picker.is_some() { return; }
                if let Some((_, i)) = self.layout.menus.iter().find(|(r, _)| Self::hit(*r, x, y)).copied() { self.menu = Some(i); self.hover_row = None; return; }
                if self.menu.is_some() {
                    match self.layout.dropdown {
                        Some((r, _)) if Self::hit(r, x, y) => self.hover_row = (y > r.y).then(|| (y - r.y) as usize - 1),
                        _ => { self.menu = None; self.hover_row = None; }
                    }
                }
            }
            MouseEventKind::ScrollUp => { let b = self.buf(); b.scroll = b.scroll.saturating_sub(3); }
            MouseEventKind::ScrollDown => { let b = self.buf(); b.scroll = (b.scroll + 3).min(b.lines().saturating_sub(1)); }
            _ => {}
        }
    }

    /// keep the primary cursor on screen
    pub fn scroll_to_cursor(&mut self) {
        let h = self.layout.text.height.max(1) as usize;
        let w = self.layout.text.width.saturating_sub(self.layout.gutter).max(1) as usize;
        let b = self.buf();
        let (l, c) = b.pos(b.primary().head);
        if l < b.scroll { b.scroll = l; }
        if l >= b.scroll + h { b.scroll = l + 1 - h; }
        let disp = display_col(&b.line_string(l), c);
        if disp < b.hscroll { b.hscroll = disp; }
        if disp >= b.hscroll + w { b.hscroll = disp + 1 - w; }
    }
}

/// screen column of char column `c` in a line (tabs are 4 wide)
pub fn display_col(line: &str, c: usize) -> usize {
    let mut d = 0;
    for ch in line.chars().take(c) { d += if ch == '\t' { 4 - d % 4 } else { 1 }; }
    d
}

fn shellexpand(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = std::env::var_os("HOME") { return format!("{}/{}", h.to_string_lossy(), rest); }
    }
    s.to_string()
}

/// "ctrl-shift-t", "alt-up", "f5", "a": the text form of a key press used by init.lua `frost.keys`
pub fn key_spec(k: &KeyEvent) -> String {
    let mut s = String::new();
    if k.modifiers.contains(KeyModifiers::CONTROL) { s.push_str("ctrl-"); }
    if k.modifiers.contains(KeyModifiers::ALT) { s.push_str("alt-"); }
    if k.modifiers.contains(KeyModifiers::SHIFT) { s.push_str("shift-"); }
    match k.code {
        KeyCode::Char(c) => s.push(c.to_ascii_lowercase()),
        KeyCode::F(n) => s.push_str(&format!("f{n}")),
        other => s.push_str(&format!("{other:?}").to_lowercase()),
    }
    s
}

/// action names for init.lua: the Debug name in lower case ("save", "reopenclosed", "nextoccurrence")
pub fn action_by_name(name: &str) -> Option<Action> {
    let all = MENUS.iter().flat_map(|m| m.1.iter().map(|i| i.2)).chain(PALETTE.iter().map(|p| p.1));
    all.into_iter().find(|a| format!("{a:?}").to_lowercase() == name)
}

#[cfg(test)]
mod key_tests {
    use super::*;
    #[test]
    fn spec_and_names() {
        let k = KeyEvent::new(KeyCode::Char('J'), KeyModifiers::CONTROL);
        assert_eq!(key_spec(&k), "ctrl-j");
        assert_eq!(key_spec(&KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)), "alt-up");
        assert!(action_by_name("goto").is_some() || action_by_name("save").is_some());
        assert_eq!(action_by_name("save"), Some(Action::Save));
        assert_eq!(action_by_name("nope"), None);
    }
}
