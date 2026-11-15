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
    Find, FindNext, FindPrev, Replace, Goto, NextOccurrence, SelectOccurrences, SkipOccurrence, UndoCursor, SplitLines, CursorUp, CursorDown, OneCursor,
    Language, Define, WordJump, WordNext, WordPrev, DiffDisk, DiffTab, Palette, ReopenClosed, ClosedTabs, SaveSession, LoadSession, ToggleLineNumbers, Help, Menu,
    // keys that are not menu commands but live in the key table all the same (so they can be rebound and are listed by F1)
    Enter, Backspace, Delete, DeleteWord,
    Left, Right, Up, Down, WordLeft, WordRight, Up5, Down5, Home, End, DocStart, DocEnd, PageUp, PageDown,
}

impl Action {
    /// moves the carets; with Shift the same key extends the selection
    pub fn is_motion(self) -> bool {
        use Action::*;
        matches!(self, Left | Right | Up | Down | WordLeft | WordRight | Up5 | Down5 | Home | End | DocStart | DocEnd | PageUp | PageDown)
    }
    /// keys that edit or move as you type: the F1 list shows them but does not run them
    pub fn is_typing(self) -> bool { self.is_motion() || matches!(self, Action::Enter | Action::Backspace | Action::Delete | Action::DeleteWord) }
}

pub const MENUS: &[(&str, &[(&str, Action)])] = &[
    ("File", &[("New", Action::New), ("Open...", Action::Open), ("Save", Action::Save), ("Save As...", Action::SaveAs),
               ("Close tab", Action::Close), ("Diff with file on disk", Action::DiffDisk), ("Diff with another tab...", Action::DiffTab), ("Reopen closed tab", Action::ReopenClosed), ("Closed tabs...", Action::ClosedTabs), ("Save session as...", Action::SaveSession), ("Load session...", Action::LoadSession), ("Quit", Action::Quit)]),
    ("Edit", &[("Undo", Action::Undo), ("Redo", Action::Redo), ("Cut", Action::Cut), ("Copy", Action::Copy), ("Paste", Action::Paste),
               ("Select all", Action::SelectAll), ("Duplicate line", Action::DuplicateLine), ("Delete line", Action::DeleteLine),
               ("Define word", Action::Define), ("Jump to word...", Action::WordJump), ("Next occurrence of word", Action::WordNext), ("Previous occurrence of word", Action::WordPrev), ("Indent", Action::Indent), ("Unindent", Action::Unindent)]),
    ("Search", &[("Find...", Action::Find), ("Find next", Action::FindNext), ("Find previous", Action::FindPrev), ("Replace all...", Action::Replace),
                 ("Go to line...", Action::Goto)]),
    ("Cursors", &[("Add next occurrence", Action::NextOccurrence), ("Select all occurrences", Action::SelectOccurrences), ("Skip this occurrence", Action::SkipOccurrence),
                  ("Add cursor above", Action::CursorUp), ("Add cursor below", Action::CursorDown), ("Cursor at the end of each selected line", Action::SplitLines),
                  ("Undo last cursor", Action::UndoCursor), ("Single cursor", Action::OneCursor)]),
    ("View", &[("Next tab", Action::NextTab), ("Previous tab", Action::PrevTab), ("Line numbers", Action::ToggleLineNumbers), ("Command palette", Action::Palette)]),
    ("Language", &[("Choose language...", Action::Language)]),
    ("Help", &[("Keys", Action::Help)]),
];

const PALETTE: &[(&str, Action)] = &[
    ("New file", Action::New), ("Open file", Action::Open), ("Save", Action::Save), ("Save as", Action::SaveAs), ("Close tab", Action::Close), ("Quit", Action::Quit),
    ("Undo", Action::Undo), ("Redo", Action::Redo), ("Select all", Action::SelectAll), ("Duplicate line", Action::DuplicateLine), ("Delete line", Action::DeleteLine),
    ("Find", Action::Find), ("Replace all", Action::Replace), ("Go to line", Action::Goto), ("Add next occurrence (multi-cursor)", Action::NextOccurrence),
    ("Select all occurrences", Action::SelectOccurrences), ("Skip this occurrence", Action::SkipOccurrence), ("Undo last cursor", Action::UndoCursor), ("Cursor at the end of each selected line", Action::SplitLines),
    ("Add cursor above", Action::CursorUp), ("Add cursor below", Action::CursorDown), ("Single cursor", Action::OneCursor), ("Choose language", Action::Language),
    ("Define word", Action::Define), ("Jump to word", Action::WordJump), ("Next occurrence of word", Action::WordNext), ("Previous occurrence of word", Action::WordPrev), ("Diff with file on disk", Action::DiffDisk), ("Diff with another tab", Action::DiffTab), ("Reopen closed tab", Action::ReopenClosed), ("Closed tabs", Action::ClosedTabs), ("Save session as", Action::SaveSession), ("Load session", Action::LoadSession), ("Toggle line numbers", Action::ToggleLineNumbers), ("Keys", Action::Help),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PromptKind { Open, SaveAs, Find, Goto, ReplaceFind, ReplaceWith, SaveSession }

pub struct Prompt { pub kind: PromptKind, pub label: String, pub input: String, pub aux: String }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerKind { Palette, Language, Sessions, Closed, DiffTab, Word, Keys }

pub struct Picker { pub kind: PickerKind, pub filter: String, pub items: Vec<String>, pub sel: usize }

impl Picker {
    pub fn shown(&self) -> Vec<usize> {
        if self.kind == PickerKind::Word { return (0..self.items.len()).collect(); }   // already ranked and filtered by the app
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
    /// when the current message appeared (it fades after a few seconds)
    msg_t: Instant,
    pub quit: bool,
    pub line_numbers: bool,
    pub layout: Layout,
    pub last_find: String,
    clip: String,
    /// what each cursor copied, so pasting the same text back at as many cursors gives everyone their own piece
    clip_parts: Vec<String>,
    /// talk to wl-copy / xclip / ...: off in tests so they never touch the real clipboard
    sys_clip: bool,
    dragging: bool,
    wheel_at: Option<std::time::Instant>,
    wheel_streak: u32,
    follow_sig: (usize, usize, usize),
    clicks: (Instant, u16, u16, u8),
    confirm_quit: bool,
    pub session_dir: PathBuf,
    disk_check: Instant,
    pub hl_key: (u64, u64),
    pub closed: Vec<TabState>,
    pub hover_row: Option<usize>,
    /// every bound key: the table in keys.rs with init.lua on top
    pub keymap: std::collections::HashMap<String, Action>,
    pub dict: crate::dict::Dict,
    /// unique words of the active buffer with the lines they occur on, alive only while the word-jump picker is open
    word_idx: Vec<(String, Vec<usize>)>,
    /// the word last jumped to (F5 / Shift-F5 repeat the jump)
    last_word: String,
    /// false when started with file arguments: the saved session is not touched
    pub persist: bool,
    /// buffer id -> history generation last written to disk
    pub hist_written: std::collections::HashMap<u64, u64>,
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
    /// an app with one empty tab and no config, session or disk access (also what the tests use)
    pub fn blank() -> App {
        let mut app = App { tabs: vec![], active: 0, hl: Highlighter::new(), prompt: None, picker: None, menu: None, msg: String::new(), quit: false, line_numbers: true,
                            layout: Layout::default(), last_find: String::new(), clip: String::new(), sys_clip: false, dragging: false, wheel_at: None, wheel_streak: 0, follow_sig: (usize::MAX, 0, 0),
                            clicks: (Instant::now(), 0, 0, 0), confirm_quit: false, session_dir: PathBuf::new(), hl_key: (0, 0), closed: vec![], hover_row: None, clip_parts: vec![], keymap: crate::keys::build(&Default::default()), dict: crate::dict::Dict::empty(), word_idx: Vec::new(), last_word: String::new(), persist: false, hist_written: Default::default(), disk_check: Instant::now(), msg_t: Instant::now() };
        app.new_tab();
        app
    }

    pub fn new(files: &[String]) -> App {
        let cfg = crate::config::load();   // first: the theme decides which syntax colours the highlighter uses
        let mut app = App::blank();
        app.tabs.clear();
        app.sys_clip = true;
        app.session_dir = session::session_dir();
        app.dict = crate::dict::Dict::load(None);
        app.line_numbers = cfg.line_numbers;
        app.keymap = crate::keys::build(&cfg.keys);
        if cfg.dictionary.is_some() { app.dict = crate::dict::Dict::load(cfg.dictionary.clone()); }
        if let Some(e) = cfg.error { app.msg = e; }
        // Notepad++ model: the session is ALWAYS kept. Whatever is open (saved files, unsaved edits, untitled tabs) comes back on the
        // next start, and closing the editor never asks and never loses work. `frost FILE...` (git, nnn, $EDITOR) restores the
        // session too and then opens/activates FILE on top of it. `file:12` and `+12 file` jump to a line.
        app.persist = true;
        app.restore(&session::load());
        let mut line = 0usize;
        for f in files {
            if let Some(n) = f.strip_prefix('+').and_then(|n| n.parse::<usize>().ok()) { line = n; continue; }
            let (path, l2) = match f.rsplit_once(':') { Some((p, n)) if n.parse::<usize>().is_ok() && !std::path::Path::new(f).exists() => (p, n.parse().unwrap_or(0)), _ => (f.as_str(), 0) };
            self_open(&mut app, path, if l2 > 0 { l2 } else { line });
            line = 0;
        }
        if app.tabs.is_empty() { app.new_tab(); }
        app
    }

    /// the message of the status bar while it is fresh; it fades after a few seconds (longer for longer text)
    pub fn msg_live(&mut self) -> Option<&str> {
        let ttl = Duration::from_millis(2500 + 60 * self.msg.chars().count() as u64);
        if !self.msg.is_empty() && self.msg_t.elapsed() > ttl { self.msg.clear(); }
        if self.msg.is_empty() { None } else { Some(&self.msg) }
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
    pub fn snapshot(&mut self) -> Session {
        let hl = &self.hl;
        Session {
            active: self.active.min(self.tabs.iter().filter(|b| b.title.is_none()).count().saturating_sub(1)),
            tabs: self.tabs.iter().filter(|b| b.title.is_none()).map(|b| Self::tab_state(hl, b)).collect(),
            closed: self.closed.clone(),
        }
    }

    fn tab_state(hl: &Highlighter, b: &Buffer) -> TabState {
        let (l, c) = b.pos(b.primary().head);
        TabState { path: b.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                   unsaved: if b.modified || b.path.is_none() { Some(b.contents()) } else { None },
                   cursor_line: l, cursor_col: c, scroll: b.scroll, syntax: Some(hl.name_of(b.syntax)),
                   history: None, hid: b.hid.clone() }
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
        if crate::history_store::valid_id(&t.hid) { b.hid = t.hid.clone(); }
        let h = crate::history_store::load(&self.session_dir, &b.hid).or_else(|| t.history.clone());
        if let Some(h) = h { if b.set_history(&h) { self.hist_written.insert(b.id, b.hist_gen); } }
        Some(b)
    }

    /// remember a tab that is being closed (not an empty untitled one)
    fn remember_closed(&mut self, i: usize) {
        let b = &self.tabs[i];
        if b.path.is_none() && b.len() == 0 { return; }
        let st = Self::tab_state(&self.hl, b);
        if self.persist && b.len() <= 4 << 20 { if let Some(h) = b.history() { let _ = crate::history_store::save(&self.session_dir, &b.hid, &h); } }
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
        // files changed by something else: one stat per tab per second
        if self.disk_check.elapsed() >= Duration::from_secs(1) {
            self.disk_check = Instant::now();
            for t in &mut self.tabs { if let Some(m) = t.sync_disk() { self.msg = m; self.msg_t = Instant::now(); } }
        }
        busy
    }

    /// `full` also writes the undo history (on exit); the 30 s autosave leaves it out because computing it costs time
    /// Write the undo history of every tab whose history changed. The 30 s autosave does it for texts up to 256 KB (so a
    /// power cut loses at most 30 s); the final save on exit does it for all (up to 4 MB). Histories nobody refers to are deleted.
    fn write_histories(&mut self, full: bool) {
        let limit = if full { 4 << 20 } else { 256 << 10 };
        for b in &self.tabs {
            if b.len() > limit || self.hist_written.get(&b.id) == Some(&b.hist_gen) { continue; }
            if let Some(h) = b.history() {
                if crate::history_store::save(&self.session_dir, &b.hid, &h).is_ok() { self.hist_written.insert(b.id, b.hist_gen); }
            }
        }
        if full {
            let keep = self.tabs.iter().map(|b| b.hid.clone()).chain(self.closed.iter().map(|c| c.hid.clone())).collect();
            crate::history_store::prune(&self.session_dir, &keep);
        }
    }

    /// cheap fingerprint of everything the periodic snapshot would store (no text copies): when it is unchanged, nothing is written
    pub fn snapshot_sig(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.active.hash(&mut h);
        for b in &self.tabs { (b.id, b.hist_gen, b.modified, b.path.as_ref()).hash(&mut h); }
        self.closed.len().hash(&mut h);
        h.finish()
    }

    pub fn save_session(&mut self, full: bool) {
        if !self.persist { return; }
        self.write_histories(full);
        let s = self.snapshot();
        if let Err(e) = session::save(&s) { eprintln!("frost: cannot save session: {e}"); }
    }

    // ---------------------------------------------------------------- clipboard
    fn copy_text(&mut self, t: String) { if self.sys_clip { set_system_clipboard(&t); } self.clip = t; }
    fn paste_text(&mut self) -> String {
        match if self.sys_clip { get_system_clipboard() } else { None } { Some(s) if !s.is_empty() => s, _ => self.clip.clone() }
    }

    // ---------------------------------------------------------------- actions
    pub fn run(&mut self, a: Action) { self.run_sel(a, false) }

    /// `select`: Shift was held, motions extend the selection
    pub fn run_sel(&mut self, a: Action, select: bool) {
        self.menu = None;
        let page = self.layout.text.height.max(2) as usize - 1;
        match a {
            Action::Enter => self.buf().newline(),
            Action::Backspace => self.buf().backspace(),
            Action::Delete => self.buf().delete(),
            Action::DeleteWord => self.buf().delete_word_back(),
            Action::Left => self.buf().move_cursors(Move::Left, select),
            Action::Right => self.buf().move_cursors(Move::Right, select),
            Action::Up => self.buf().move_cursors(Move::Up, select),
            Action::Down => self.buf().move_cursors(Move::Down, select),
            Action::WordLeft => self.buf().move_cursors(Move::WordLeft, select),
            Action::WordRight => self.buf().move_cursors(Move::WordRight, select),
            Action::Up5 => for _ in 0..5 { self.buf().move_cursors(Move::Up, select) },
            Action::Down5 => for _ in 0..5 { self.buf().move_cursors(Move::Down, select) },
            Action::Home => self.buf().move_cursors(Move::Home, select),
            Action::End => self.buf().move_cursors(Move::End, select),
            Action::DocStart => self.buf().move_cursors(Move::DocStart, select),
            Action::DocEnd => self.buf().move_cursors(Move::DocEnd, select),
            Action::PageUp => self.buf().move_cursors(Move::PageUp(page), select),
            Action::PageDown => self.buf().move_cursors(Move::PageDown(page), select),
            Action::Menu => self.menu = Some(0),
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
            Action::WordJump => {
                self.build_word_idx();
                if self.word_idx.is_empty() { self.msg = "no words in this tab".into(); } else {
                    self.picker = Some(Picker { kind: PickerKind::Word, filter: String::new(), items: vec![], sel: 0 });
                    self.refresh_words();
                }
            }
            Action::WordNext | Action::WordPrev => {
                if self.last_word.is_empty() { let (w, _) = self.buf().word_at_caret(); self.last_word = w; }
                let w = self.last_word.clone();
                if w.is_empty() { self.msg = "F4 picks a word first".into(); } else if !self.jump_word(&w, a == Action::WordNext) { self.msg = format!("'{w}' is not in this tab"); }
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
            Action::Undo => self.msg = if self.buf().undo() { "undone".into() } else { "nothing to undo".into() },
            Action::Redo => self.msg = if self.buf().redo() { "redone".into() } else { "nothing to redo".into() },
            Action::Cut => {
                let what = self.buf().copy_kind();
                let parts = self.buf().selection_parts();
                let t = self.buf().cut();
                self.clip_parts = parts;
                self.msg = format!("cut {}", what.describe(&t));
                self.copy_text(t);
            }
            Action::Copy => {
                let what = self.buf().copy_kind();
                self.clip_parts = self.buf().selection_parts();
                let t = self.buf().selected_text();
                self.msg = format!("copied {}", what.describe(&t));
                self.copy_text(t);
            }
            Action::Paste => { let t = self.paste_text(); self.paste(&t); }
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
            Action::NextOccurrence => {
                let added = self.buf().add_next_occurrence();   // the status bar counts the cursors
                if !added { self.msg = "no more occurrences".into(); }
            }
            Action::SelectOccurrences => {
                let n = self.buf().select_all_occurrences();
                self.msg = if n == 0 { "nothing to select".into() } else { format!("{n} occurrences selected") };
            }
            Action::SkipOccurrence => {
                if !self.buf().skip_occurrence() { self.msg = "no other occurrence".into(); }
            }
            Action::UndoCursor => {
                let ok = self.buf().drop_newest_cursor();
                let n = self.buf().cursors.len();
                if !ok { self.msg = "only one cursor".into(); } else if n == 1 { self.msg = "back to one cursor".into(); }
            }
            Action::SplitLines => {
                let n = self.buf().cursors_at_line_ends();
                self.msg = if n == 0 { "select several lines first".into() } else { format!("{n} cursors, one per line") };
            }
            Action::CursorUp => self.buf().add_cursor_vertical(false),
            Action::CursorDown => self.buf().add_cursor_vertical(true),
            Action::OneCursor => { if self.buf().cursors.len() > 1 { self.msg = "back to one cursor".into(); } self.buf().collapse() }
            Action::Language => self.picker = Some(Picker { kind: PickerKind::Language, filter: String::new(), items: { let mut n = self.hl.language_names(); n.sort_by_key(|s| s.to_lowercase()); n.dedup(); n }, sel: 0 }),
            Action::Palette => self.picker = Some(Picker { kind: PickerKind::Palette, filter: String::new(), items: PALETTE.iter().map(|p| p.0.to_string()).collect(), sel: 0 }),
            Action::SaveSession => self.prompt = Some(Prompt { kind: PromptKind::SaveSession, label: "Save session as:".into(), input: String::new(), aux: String::new() }),
            Action::LoadSession => {
                let names = session::list_named(&self.session_dir);
                if names.is_empty() { self.msg = "no saved sessions".into(); } else { self.picker = Some(Picker { kind: PickerKind::Sessions, filter: String::new(), items: names, sel: 0 }); }
            }
            Action::ToggleLineNumbers => self.line_numbers = !self.line_numbers,
            Action::Help => {
                let items = crate::keys::overview(&self.keymap).into_iter().map(|r| r.0).collect();
                self.picker = Some(Picker { kind: PickerKind::Keys, filter: String::new(), items, sel: 1 });
            }
        }
    }

    /// paste `t` at every cursor and say what happened
    pub fn paste(&mut self, t: &str) {
        if t.is_empty() { self.msg = "nothing to paste".into(); return; }
        let b = self.buf();
        let n = b.cursors.len();
        if n > 1 && self.clip_parts.len() == n && self.clip_parts.join("\n") == t {
            let parts = std::mem::take(&mut self.clip_parts);
            self.buf().paste_parts(&parts);
            self.clip_parts = parts;
            self.msg = format!("pasted {n} pieces, one per cursor");
            return;
        }
        let b = self.buf();
        let spread = b.paste(t);
        self.msg = if spread { format!("pasted {} lines, one per cursor", n) } else if n > 1 { format!("pasted {} at {n} cursors", amount(t)) } else { format!("pasted {}", amount(t)) };
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
    /// run `f`; a message it sets restarts the fade timer, otherwise the previous message keeps counting down
    /// the active tab follows the language the highlighter picked: pairs, indentation and strictness depend on it
    pub fn sync_lang(&mut self) {
        let i = self.active.min(self.tabs.len() - 1);
        let name = self.hl.name_of(self.tabs[i].syntax);
        if self.tabs[i].lang.name != name { self.tabs[i].set_lang(crate::lang::Lang::of(&name)); }
    }

    fn keeping_msg(&mut self, f: impl FnOnce(&mut Self)) {
        let old = std::mem::take(&mut self.msg);
        self.sync_lang();
        f(self);
        if let Some(n) = self.buf().notice.take() { self.msg = n; }
        if self.msg.is_empty() { self.msg = old; } else { self.msg_t = Instant::now(); }
    }

    pub fn on_paste(&mut self, t: &str) { self.keeping_msg(|a| a.paste(t)); }

    pub fn on_key(&mut self, k: KeyEvent) {
        self.keeping_msg(|a| a.on_key_inner(k));
        if self.picker.is_none() && !self.word_idx.is_empty() { self.word_idx = Vec::new(); }
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

    /// the action a key press is bound to (and whether Shift only extends a motion)
    fn lookup(&self, k: &KeyEvent) -> Option<(Action, bool)> {
        let spec = key_spec(k);
        if let Some(&a) = self.keymap.get(&spec) { return Some((a, false)); }
        if k.modifiers.contains(KeyModifiers::SHIFT) {
            if let Some(&a) = self.keymap.get(&spec.replace("shift-", "")) { if a.is_motion() { return Some((a, true)); } }
        }
        None
    }

    /// where the user is, for the hint line
    pub fn hint_ctx(&mut self) -> crate::keys::Ctx {
        use crate::keys::Ctx;
        if self.menu.is_some() { return Ctx::Menu; }
        if let Some(p) = &self.picker { return Ctx::Picker { keys: p.kind == PickerKind::Keys }; }
        if let Some(p) = &self.prompt { return if p.kind == PromptKind::Find { Ctx::Find } else { Ctx::Prompt }; }
        let b = self.buf();
        Ctx::Edit { cursors: b.cursors.len(), selected: b.cursors.iter().any(|c| c.has_sel()) }
    }

    fn on_key_inner(&mut self, k: KeyEvent) {
        self.confirm_quit = false;
        if self.picker.is_some() { return self.picker_key(k); }
        if self.prompt.is_some() { return self.prompt_key(k); }
        if let Some(m) = self.menu { return self.menu_key(k, m); }
        if let Some((a, select)) = self.lookup(&k) { return self.run_sel(a, select); }
        if let KeyCode::Char(c) = k.code {
            if !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) { self.buf().type_char(c); }
        }
    }

    fn menu_key(&mut self, k: KeyEvent, m: usize) {
        match k.code {
            KeyCode::Esc => self.menu = None,
            KeyCode::Left => self.menu = Some((m + MENUS.len() - 1) % MENUS.len()),
            KeyCode::Right => self.menu = Some((m + 1) % MENUS.len()),
            KeyCode::Char(c) => {
                // first letter of an item runs it
                if let Some(it) = MENUS[m].1.iter().find(|i| i.0.to_lowercase().starts_with(c.to_ascii_lowercase())) { self.run(it.1); }
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
                let s = self.snapshot();
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
            KeyCode::Backspace => { pk.filter.pop(); pk.sel = 0; if pk.kind == PickerKind::Word { self.refresh_words(); } }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => { pk.filter.push(c); pk.sel = 0; if pk.kind == PickerKind::Word { self.refresh_words(); } }
            KeyCode::Enter => {
                let pk = self.picker.take().unwrap();
                if let Some(&i) = pk.shown().get(pk.sel) { self.picked(pk.kind, &pk.items[i].clone()); }
            }
            _ => {}
        }
    }

    fn build_word_idx(&mut self) {
        let b = self.buf();
        b.ensure_loaded();
        let mut map: std::collections::HashMap<String, Vec<usize>> = Default::default();
        let mut seen = 0usize;
        'lines: for i in 0..b.lines() {
            for w in b.text.line(i).to_string().split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if w.chars().count() < 2 { continue; }
                seen += w.len();
                let v = map.entry(w.to_string()).or_default();
                if v.last() != Some(&i) && v.len() < 5000 { v.push(i); }
                if map.len() > 200_000 || seen > 64 << 20 { break 'lines; }
            }
        }
        self.word_idx = map.into_iter().collect();
    }

    /// rank the buffer words (and dictionary words that are not in the text) against the picker filter: prefix, substring, fuzzy
    fn refresh_words(&mut self) {
        let Some(pk) = self.picker.as_ref().filter(|p| p.kind == PickerKind::Word) else { return };
        let f = pk.filter.to_lowercase();
        let cur = { let b = &self.tabs[self.active.min(self.tabs.len() - 1)]; b.pos(b.primary().head).0 };
        let next_line = |ls: &Vec<usize>| ls.iter().copied().find(|&l| l >= cur).unwrap_or(ls[0]);
        let dist = |ls: &Vec<usize>| { let n = next_line(ls); if n >= cur { n - cur } else { usize::MAX / 2 + cur - n } };
        let mut scored: Vec<(u8, usize, usize, usize)> = vec![];   // (score, distance, length, index)
        for (i, (w, ls)) in self.word_idx.iter().enumerate() {
            let lw = w.to_lowercase();
            let sc = if f.is_empty() || lw.starts_with(&f) { 0 } else if lw.contains(&f) { 1 } else {
                let mut it = lw.chars(); if f.chars().all(|c| it.any(|x| x == c)) { 2 } else { continue }
            };
            scored.push((sc, dist(ls), w.chars().count(), i));
        }
        scored.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)).then_with(|| self.word_idx[a.3].0.cmp(&self.word_idx[b.3].0)));
        let mut items: Vec<String> = scored.iter().take(300).map(|&(_, _, _, i)| {
            let (w, ls) = &self.word_idx[i];
            format!("{w}  line {}  x{}", next_line(ls) + 1, ls.len())
        }).collect();
        if f.chars().count() >= 2 {
            let have: std::collections::HashSet<String> = self.word_idx.iter().map(|e| e.0.to_lowercase()).collect();
            for (w, m) in self.dict.complete(&f, &[], 40).into_iter().filter(|(w, _)| !have.contains(&w.to_lowercase())).take(12) {
                items.push(if m.is_empty() { format!("{w}  - dictionary") } else { format!("{w}  - {m}") });
            }
        }
        if let Some(pk) = self.picker.as_mut() { pk.items = items; pk.sel = 0; }
    }

    /// move the caret onto the next (or previous) whole-word occurrence of `w`, wrapping around; the word is selected
    fn jump_word(&mut self, w: &str, forward: bool) -> bool {
        let b = self.buf();
        b.ensure_loaded();
        let n = b.lines();
        let head = b.primary();
        let cl = b.pos(if forward { head.end() } else { head.start() }).0;
        let occ = |line: usize| -> Vec<(usize, usize)> {
            let t = b.text.line(line).to_string();
            let base = b.text.line_to_char(line);
            let wl = w.chars().count();
            let mut v = vec![];
            let mut start: Option<usize> = None;
            let mut chars: Vec<char> = t.chars().collect();
            chars.push('\n');   // sentinel closes the last word
            for (i, c) in chars.iter().enumerate() {
                let wc = c.is_alphanumeric() || *c == '_';
                if wc && start.is_none() { start = Some(i); }
                if !wc { if let Some(s0) = start.take() { if i - s0 == wl && chars[s0..i].iter().copied().eq(w.chars()) { v.push((base + s0, base + i)); } } }
            }
            v
        };
        for step in 0..=n {
            let line = if forward { (cl + step) % n } else { (cl + n - step % n) % n };
            let mut o = occ(line);
            if !forward { o.reverse(); }
            for (s, e) in o {
                let ok = if step == 0 { if forward { s >= head.end() } else { e <= head.start() } } else if step == n { if forward { s < head.end() } else { e > head.start() } } else { true };
                if ok {
                    b.cursors = vec![crate::buffer::Cursor { head: e, anchor: s, want: None }];
                    return true;
                }
            }
        }
        false
    }

    fn picked(&mut self, kind: PickerKind, item: &str) {
        match kind {
            PickerKind::Word => {
                let w = item.split_whitespace().next().unwrap_or("").to_string();
                let in_text = self.word_idx.iter().any(|e| e.0 == w);
                if in_text {
                    self.last_word = w.clone();
                    if !self.jump_word(&w, true) { self.msg = format!("'{w}' not found"); }
                } else if let Some(m) = self.dict.define(&w) { self.msg = format!("{w}: {m} (not in this tab)"); } else { self.msg = format!("'{w}' is not in this tab"); }
            }
            PickerKind::Keys => {
                // Enter on a row runs that command (movement and typing keys are only listed)
                if let Some((_, Some(a))) = crate::keys::overview(&self.keymap).into_iter().find(|r| r.0 == item) { if !a.is_typing() { self.run(a); } }
            }
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
            let w = if ch == '\t' { crate::config::tab_width() - disp % crate::config::tab_width() } else { 1 };
            if disp + w > want + (w / 2) && disp >= want || disp + w > want { break; }
            disp += w;
            col += 1;
        }
        b.idx(line, col)
    }

    pub fn on_mouse(&mut self, m: MouseEvent) { self.keeping_msg(|a| a.on_mouse_inner(m)); }

    fn on_mouse_inner(&mut self, m: MouseEvent) {
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
                        if row >= 1 { if let Some(it) = MENUS[mi].1.get(row - 1) { self.run(it.1); } }
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
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown | MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                // the faster the wheel turns, the further each notch goes (3 lines, up to 24); Ctrl = a page
                let now = std::time::Instant::now();
                self.wheel_streak = match self.wheel_at { Some(t) if now.duration_since(t).as_millis() < 120 => (self.wheel_streak + 1).min(21), _ => 0 };
                self.wheel_at = Some(now);
                let page = self.layout.text.height.max(1) as usize;
                let mut step = 3 * (1 + self.wheel_streak as usize / 3);
                if m.modifiers.contains(KeyModifiers::CONTROL) { step = page; }
                let sideways = matches!(m.kind, MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight) || m.modifiers.contains(KeyModifiers::SHIFT);
                let up = matches!(m.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollLeft);
                let b = self.buf();
                if sideways { b.hscroll = if up { b.hscroll.saturating_sub(step) } else { b.hscroll + step }; }
                else if up { b.scroll = b.scroll.saturating_sub(step); }
                else { b.scroll = (b.scroll + step).min(b.lines().saturating_sub(1)); }
            }
            _ => {}
        }
    }

    /// scroll the view by `d` lines without touching the cursor (Ctrl+Up/Down)
    pub fn scroll_view(&mut self, d: isize) {
        let b = self.buf();
        let last = b.lines().saturating_sub(1);
        b.scroll = if d < 0 { b.scroll.saturating_sub((-d) as usize) } else { (b.scroll + d as usize).min(last) };
    }

    /// keep the primary cursor on screen
    pub fn scroll_to_cursor(&mut self) {
        // only follow the cursor when it (or the text) moved, otherwise the wheel could never scroll away from it
        let sig = { let a = self.active; let b = self.buf(); (a, b.primary().head, b.text.len_chars()) };
        if sig == self.follow_sig { return; }
        self.follow_sig = sig;
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

/// "12 chars", "1 line", "3 lines": what a piece of text amounts to
pub fn amount(t: &str) -> String {
    let lines = t.lines().count();
    if t.contains('\n') && lines > 1 || t.ends_with('\n') { return format!("{lines} line{}", if lines == 1 { "" } else { "s" }); }
    let n = t.chars().count();
    format!("{n} char{}", if n == 1 { "" } else { "s" })
}

/// screen column of char column `c` in a line (tabs are 4 wide)
pub fn display_col(line: &str, c: usize) -> usize {
    let mut d = 0;
    let tab = crate::config::tab_width();
    for ch in line.chars().take(c) { d += if ch == '\t' { tab - d % tab } else { 1 }; }
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
    let all = MENUS.iter().flat_map(|m| m.1.iter().map(|i| i.1)).chain(PALETTE.iter().map(|p| p.1)).chain(crate::keys::KEYS.iter().map(|r| r.act));
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

fn self_open(app: &mut App, path: &str, line: usize) {
    app.open_path(PathBuf::from(path));
    if line > 0 { app.buf().goto_line(line); }
}

#[cfg(test)]
mod msg_tests {
    use super::*;

    fn app(text: &str) -> App {
        let mut a = App::blank();
        a.tabs[0] = Buffer::from_text(text, None);
        a
    }
    fn sel(a: &mut App, from: usize, to: usize) { a.buf().cursors = vec![crate::buffer::Cursor { head: to, anchor: from, want: None }]; }

    #[test]
    fn amounts() {
        assert_eq!(amount("a"), "1 char");
        assert_eq!(amount("abc"), "3 chars");
        assert_eq!(amount("a\n"), "1 line");
        assert_eq!(amount("a\nb\n"), "2 lines");
        assert_eq!(amount("a\nb"), "2 lines");
        assert_eq!(amount("zażółć"), "6 chars");
    }

    #[test]
    fn copy_cut_paste_say_what_they_did() {
        let mut a = app("hello world\nsecond");
        sel(&mut a, 0, 5);
        a.run(Action::Copy);
        assert_eq!(a.msg, "copied 5 chars");
        a.run(Action::Cut);
        assert_eq!(a.msg, "cut 5 chars");
        assert_eq!(a.buf().contents(), " world\nsecond");
        a.buf().collapse();
        a.run(Action::Paste);
        assert_eq!(a.msg, "pasted 5 chars");
        assert_eq!(a.buf().contents(), "hello world\nsecond");
        a.buf().cursors = vec![crate::buffer::Cursor::at(2)];
        a.run(Action::Copy);
        assert_eq!(a.msg, "copied 1 line");
        a.run(Action::Cut);
        assert_eq!(a.msg, "cut 1 line");
    }

    #[test]
    fn multi_cursor_copy_and_paste_messages() {
        let mut a = app("aa bb cc");
        a.buf().cursors = vec![crate::buffer::Cursor { head: 2, anchor: 0, want: None }, crate::buffer::Cursor { head: 8, anchor: 6, want: None }];
        a.run(Action::Copy);
        assert_eq!(a.msg, "copied 2 selections");
        a.buf().collapse();
        a.buf().cursors = vec![crate::buffer::Cursor::at(2), crate::buffer::Cursor::at(5)];
        a.run(Action::Paste);
        assert_eq!(a.msg, "pasted 2 pieces, one per cursor");
        assert_eq!(a.buf().contents(), "aaaa bbcc cc");
        // text that did not come from this editor is still spread by lines
        a.buf().cursors = vec![crate::buffer::Cursor::at(0), crate::buffer::Cursor::at(1)];
        a.paste("x\ny");
        assert_eq!(a.msg, "pasted 2 lines, one per cursor");
    }

    #[test]
    fn nothing_to_paste_and_undo_redo() {
        let mut a = app("x");
        a.run(Action::Paste);
        assert_eq!(a.msg, "nothing to paste");
        a.run(Action::Undo);
        assert_eq!(a.msg, "nothing to undo");
        a.buf().insert("y");
        a.run(Action::Undo);
        assert_eq!(a.msg, "undone");
        a.run(Action::Redo);
        assert_eq!(a.msg, "redone");
        a.run(Action::Redo);
        assert_eq!(a.msg, "nothing to redo");
    }

    #[test]
    fn saved_message_names_the_file() {
        let p = std::env::temp_dir().join(format!("frost-msg-{}.txt", std::process::id()));
        let mut a = app("data\n");
        a.buf().path = Some(p.clone());
        a.run(Action::Save);
        assert!(a.msg.starts_with("saved frost-msg-"), "{}", a.msg);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn messages_fade_and_a_repeat_restarts_the_clock() {
        let mut a = app("abc");
        a.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));   // select all
        a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(a.msg_live(), Some("copied 3 chars"));
        a.msg_t = Instant::now() - Duration::from_secs(2);
        assert!(a.msg_live().is_some(), "still there after 2 s");
        // typing does not clear it early and does not keep it alive either
        a.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(a.msg, "copied 3 chars");
        a.msg_t = Instant::now() - Duration::from_secs(10);
        assert_eq!(a.msg_live(), None, "faded");
        // the same message again is fresh again
        a.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.msg_live().is_some());
        a.msg_t = Instant::now() - Duration::from_secs(10);
        a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.msg_t.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn bracketed_paste_reports_too() {
        let mut a = app("");
        a.on_paste("one\ntwo\n");
        assert_eq!(a.msg, "pasted 2 lines");
        a.on_paste("");
        assert_eq!(a.msg, "nothing to paste");
    }

    fn key(a: &mut App, code: KeyCode, m: KeyModifiers) { a.on_key(KeyEvent::new(code, m)); }

    #[test]
    fn multicursor_commands_report_and_work_from_the_keyboard() {
        let mut a = app("foo bar foo baz foo");
        key(&mut a, KeyCode::Char('d'), KeyModifiers::CONTROL);          // the word
        key(&mut a, KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(a.buf().cursors.len(), 2);
        key(&mut a, KeyCode::Char('s'), KeyModifiers::ALT);              // skip the second, take the third
        assert_eq!((a.buf().cursors.len(), a.buf().primary().start()), (2, 16));
        key(&mut a, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(a.msg, "back to one cursor");
        key(&mut a, KeyCode::Char('a'), KeyModifiers::ALT);
        assert_eq!(a.msg, "3 occurrences selected");
        key(&mut a, KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(a.buf().contents(), "x bar x baz x");
        key(&mut a, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(a.msg, "back to one cursor");
        assert_eq!(a.buf().cursors.len(), 1);
    }

    #[test]
    fn split_selected_lines_and_the_hint_line_for_cursors() {
        let mut a = app("a\nb\nc");
        key(&mut a, KeyCode::Char('l'), KeyModifiers::ALT);
        assert_eq!(a.msg, "select several lines first");
        sel(&mut a, 0, 5);
        key(&mut a, KeyCode::Char('l'), KeyModifiers::ALT);
        assert_eq!(a.msg, "3 cursors, one per line");
        let h = crate::keys::hints(&crate::keys::Ctx::Edit { cursors: 3, selected: false }, &a.keymap);
        assert!(h.iter().any(|(k, d)| k == "Alt-S" && d == "skip"), "{h:?}");
    }

    #[test]
    fn each_cursor_gets_its_own_piece_back() {
        let mut a = app("ab\ncd\n12\n34");
        a.buf().cursors = vec![crate::buffer::Cursor { head: 2, anchor: 0, want: None }, crate::buffer::Cursor { head: 5, anchor: 3, want: None }];
        a.run(Action::Copy);
        assert_eq!(a.msg, "copied 2 selections");
        a.buf().cursors = vec![crate::buffer::Cursor::at(6), crate::buffer::Cursor::at(9)];
        a.run(Action::Paste);
        assert_eq!(a.msg, "pasted 2 pieces, one per cursor");
        assert_eq!(a.buf().contents(), "ab\ncd\nab12\ncd34");
    }
}
