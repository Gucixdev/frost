//! The one keymap table. Key dispatch, the menu shortcut labels, the hint line and the F1 overview are all generated from
//! `KEYS`, so none of them can disagree with what a key really does. `frost.keys["ctrl-j"] = "goto"` in init.lua edits the
//! effective map (`"none"` unbinds), and the overview follows.
use crate::app::Action::{self, *};
use std::collections::HashMap;

/// `keys` are space separated alternatives ("ctrl-p ctrl-e"), spelled like `app::key_spec` prints them.
pub struct Bind { pub keys: &'static str, pub act: Action, pub short: &'static str, pub desc: &'static str, pub group: &'static str }

const fn b(keys: &'static str, act: Action, short: &'static str, desc: &'static str, group: &'static str) -> Bind { Bind { keys, act, short, desc, group } }

pub const KEYS: &[Bind] = &[
    b("ctrl-s", Save, "save", "Save", "File"),
    b("ctrl-shift-s", SaveAs, "save as", "Save as...", "File"),
    b("ctrl-o", Open, "open", "Open file...", "File"),
    b("ctrl-n", New, "new", "New tab", "File"),
    b("ctrl-w", Close, "close", "Close tab", "File"),
    b("ctrl-t", ReopenClosed, "reopen", "Reopen the last closed tab", "File"),
    b("ctrl-q", Quit, "quit", "Quit (the session is kept)", "File"),
    b("ctrl-pageup", PrevTab, "prev tab", "Previous tab", "File"),
    b("ctrl-pagedown", NextTab, "next tab", "Next tab", "File"),

    b("ctrl-z", Undo, "undo", "Undo", "Edit"),
    b("ctrl-y ctrl-shift-z", Redo, "redo", "Redo", "Edit"),
    b("ctrl-x", Cut, "cut", "Cut (no selection: the line)", "Edit"),
    b("ctrl-c", Copy, "copy", "Copy (no selection: the line)", "Edit"),
    b("ctrl-v", Paste, "paste", "Paste (a line per cursor)", "Edit"),
    b("ctrl-a", SelectAll, "select all", "Select all", "Edit"),
    b("ctrl-k", DeleteLine, "del line", "Delete line", "Edit"),
    b("ctrl-shift-d", DuplicateLine, "duplicate", "Duplicate line", "Edit"),
    b("tab", Indent, "indent", "Indent (insert a tab)", "Edit"),
    b("shift-backtab", Unindent, "unindent", "Unindent", "Edit"),
    b("enter", Enter, "newline", "New line, keeps the indentation", "Edit"),
    b("backspace", Backspace, "", "Delete the character before", "Edit"),
    b("delete", Delete, "", "Delete the character after", "Edit"),
    b("ctrl-backspace", DeleteWord, "", "Delete the word before", "Edit"),

    b("ctrl-f", Find, "find", "Find (live)", "Search"),
    b("f3", FindNext, "next", "Find next", "Search"),
    b("shift-f3", FindPrev, "prev", "Find previous", "Search"),
    b("ctrl-h", Replace, "replace", "Replace all...", "Search"),
    b("ctrl-g", Goto, "goto", "Go to line...", "Search"),
    b("f4", WordJump, "words", "Jump to a word of the text or the dictionary", "Search"),
    b("f5", WordNext, "", "Next occurrence of the word", "Search"),
    b("shift-f5", WordPrev, "", "Previous occurrence of the word", "Search"),
    b("f2", Define, "define", "Explain the word under the caret", "Search"),

    b("ctrl-d", NextOccurrence, "next match", "Select the word, then the next match as a cursor", "Cursors"),
    b("alt-a", SelectOccurrences, "all matches", "Select every occurrence of the selection (or word)", "Cursors"),
    b("alt-s", SkipOccurrence, "skip", "Skip this occurrence, take the next", "Cursors"),
    b("ctrl-u", UndoCursor, "undo cursor", "Drop the newest cursor", "Cursors"),
    b("alt-l", SplitLines, "split lines", "One cursor at the end of each selected line", "Cursors"),
    b("alt-up", CursorUp, "cursor up", "Add a cursor above", "Cursors"),
    b("alt-down", CursorDown, "cursor down", "Add a cursor below", "Cursors"),
    b("esc", OneCursor, "one cursor", "Back to one cursor, drop the selection", "Cursors"),

    b("left", Left, "", "Move left (Shift: select)", "Move"),
    b("right", Right, "", "Move right (Shift: select)", "Move"),
    b("up", Up, "", "Move up (Shift: select)", "Move"),
    b("down", Down, "", "Move down (Shift: select)", "Move"),
    b("ctrl-left", WordLeft, "", "Word left (Shift: select)", "Move"),
    b("ctrl-right", WordRight, "", "Word right (Shift: select)", "Move"),
    b("ctrl-up", Up5, "", "Five lines up (Shift: select)", "Move"),
    b("ctrl-down", Down5, "", "Five lines down (Shift: select)", "Move"),
    b("home", Home, "", "Line start, first the first letter (Shift: select)", "Move"),
    b("end", End, "", "End of the line (Shift: select)", "Move"),
    b("ctrl-home", DocStart, "", "Start of the text (Shift: select)", "Move"),
    b("ctrl-end", DocEnd, "", "End of the text (Shift: select)", "Move"),
    b("pageup", PageUp, "", "Page up (Shift: select)", "Move"),
    b("pagedown", PageDown, "", "Page down (Shift: select)", "Move"),

    b("ctrl-l", Language, "language", "Choose the language of the tab", "View"),
    b("ctrl-p ctrl-e", Palette, "commands", "Command palette", "View"),
    b("f10", Menu, "menu", "Open the menu bar", "View"),
    b("f1", Help, "all keys", "This list of keys", "View"),
];

/// effective key -> action map: the table, then init.lua on top ("none" unbinds a key)
pub fn build(user: &HashMap<String, String>) -> HashMap<String, Action> {
    let mut m: HashMap<String, Action> = HashMap::new();
    for r in KEYS { for k in r.keys.split_whitespace() { m.insert(k.to_string(), r.act); } }
    for (k, v) in user {
        if v == "none" { m.remove(k); } else if let Some(a) = crate::app::action_by_name(v) { m.insert(k.clone(), a); }
    }
    m
}

/// "ctrl-shift-s" -> "Ctrl-Shift-S", "pagedown" -> "PgDn", "shift-backtab" -> "Shift-Tab"
pub fn label(spec: &str) -> String {
    spec.split('-').filter(|p| !p.is_empty()).map(|p| match p {
        "pageup" => "PgUp".to_string(), "pagedown" => "PgDn".to_string(), "backtab" => "Tab".to_string(), "esc" => "Esc".to_string(),
        p if p.len() > 1 && p.starts_with('f') && p[1..].chars().all(|c| c.is_ascii_digit()) => p.to_uppercase(),
        p => { let mut c = p.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default() }
    }).collect::<Vec<_>>().join("-")
}

/// the first key bound to `a` (table order), for menus and hints
pub fn key_of(m: &HashMap<String, Action>, a: Action) -> Option<String> {
    let rank = |k: &str| KEYS.iter().enumerate().find_map(|(i, r)| r.keys.split_whitespace().position(|x| x == k).map(|j| (i, j))).unwrap_or((usize::MAX, 0));
    m.iter().filter(|(_, x)| **x == a).map(|(k, _)| k).min_by_key(|k| (rank(k), k.len(), (*k).clone())).map(|k| label(k))
}

/// the F1 list: (text, action) rows, a group heading before each group. Built from the effective map, so a key that
/// init.lua rebound is shown with its new meaning and nothing that is bound can be missing.
pub fn overview(m: &HashMap<String, Action>) -> Vec<(String, Option<Action>)> {
    let mut out: Vec<(String, Option<Action>)> = vec![];
    let mut seen: Vec<&str> = vec![];
    let mut group = "";
    for r in KEYS {
        let mine: Vec<String> = r.keys.split_whitespace().filter(|k| m.get(*k) == Some(&r.act)).map(label).collect();
        seen.extend(r.keys.split_whitespace());
        if mine.is_empty() { continue; }
        if r.group != group { group = r.group; out.push((format!("-- {group}"), None)); }
        out.push((format!("{:<22} {}", mine.join(" / "), r.desc), Some(r.act)));
    }
    let mut extra: Vec<(&String, &Action)> = m.iter().filter(|(k, a)| !KEYS.iter().any(|r| r.act == **a && r.keys.split_whitespace().any(|x| x == k.as_str()))).collect();
    extra.sort_by(|a, b| a.0.cmp(b.0));
    if !extra.is_empty() { out.push(("-- init.lua".into(), None)); }
    for (k, a) in extra { out.push((format!("{:<22} {}", label(k), format!("{a:?}").to_lowercase()), Some(*a))); }
    out
}

/// what the hint line offers: (key, short name) pairs in this order, first the keys that matter most right now
pub fn hints(ctx: &Ctx, m: &HashMap<String, Action>) -> Vec<(String, String)> {
    let of = |acts: &[Action]| -> Vec<(String, String)> {
        acts.iter().filter_map(|a| {
            let key = key_of(m, *a)?;
            let short = KEYS.iter().find(|r| r.act == *a).map(|r| r.short).filter(|s| !s.is_empty())?;
            Some((key, short.to_string()))
        }).collect()
    };
    let lit = |v: &[(&str, &str)]| v.iter().map(|(k, d)| (k.to_string(), d.to_string())).collect::<Vec<_>>();
    match ctx {
        Ctx::Menu => lit(&[("Left/Right", "menu"), ("letter", "run item"), ("Esc", "close")]),
        Ctx::Picker { keys: true } => lit(&[("type", "filter"), ("Up/Down", "choose"), ("Enter", "run it"), ("Esc", "close")]),
        Ctx::Picker { .. } => lit(&[("type", "filter"), ("Up/Down", "choose"), ("Enter", "select"), ("Esc", "close")]),
        Ctx::Find => lit(&[("Enter", "find"), ("F3", "next"), ("Shift-F3", "prev"), ("Esc", "cancel")]),
        Ctx::Prompt => lit(&[("Enter", "ok"), ("Esc", "cancel")]),
        Ctx::Edit { cursors, .. } if *cursors > 1 => {
            let mut v = vec![("Esc".to_string(), "one cursor".to_string()), ("type".to_string(), format!("edits all {cursors}"))];
            v.extend(of(&[NextOccurrence, SkipOccurrence, UndoCursor, CursorUp, Paste]));
            v
        }
        Ctx::Edit { selected: true, .. } => of(&[Copy, Cut, NextOccurrence, SelectOccurrences, Indent, Find, OneCursor]),
        Ctx::Edit { .. } => of(&[Help, Save, Find, Undo, NextOccurrence, Palette, Quit]),
    }
}

/// where the user is, for the hint line
pub enum Ctx { Menu, Picker { keys: bool }, Find, Prompt, Edit { cursors: usize, selected: bool } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn no_key_is_bound_twice_in_the_table() {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for r in KEYS { for k in r.keys.split_whitespace() { assert!(seen.insert(k, r.desc).is_none(), "{k} is bound twice"); } }
    }

    #[test]
    fn every_table_key_is_spelled_the_way_key_spec_prints_it() {
        // a typo in the table would be a key that can never fire
        for r in KEYS { for k in r.keys.split_whitespace() {
            let parts: Vec<&str> = k.split('-').collect();
            let (mods, code) = parts.split_at(parts.len() - 1);
            assert!(mods.iter().all(|m| ["ctrl", "alt", "shift"].contains(m)), "{k}");
            assert!(mods.windows(2).all(|w| ["ctrl", "alt", "shift"].iter().position(|x| x == &w[0]) < ["ctrl", "alt", "shift"].iter().position(|x| x == &w[1])), "{k}: modifier order is ctrl, alt, shift");
            let c = code[0];
            assert!(c.len() == 1 && c.chars().all(|ch| ch.is_ascii_lowercase()) || ["left", "right", "up", "down", "home", "end", "pageup", "pagedown", "tab", "backtab", "esc", "enter", "backspace", "delete"].contains(&c) || (c.starts_with('f') && c[1..].parse::<u8>().is_ok()), "{k}");
        } }
    }

    #[test]
    fn the_overview_shows_every_bound_key() {
        let mut user = HashMap::new();
        user.insert("ctrl-j".to_string(), "goto".to_string());     // an extra binding
        user.insert("ctrl-q".to_string(), "none".to_string());     // an unbound default
        user.insert("ctrl-k".to_string(), "save".to_string());     // a rebound default
        for map in [build(&HashMap::new()), build(&user)] {
            let text = overview(&map).into_iter().map(|r| r.0).collect::<Vec<_>>().join("\n");
            for (spec, act) in &map {
                let lab = label(spec);
                let row = text.lines().find(|l| l.split(" / ").any(|p| p.trim_start().starts_with(&lab) && { let rest = &p.trim_start()[lab.len()..]; rest.is_empty() || rest.starts_with(' ') }));
                assert!(row.is_some(), "{spec} ({act:?}) is bound but the overview does not show it:\n{text}");
            }
        }
        let map = build(&user);
        let text = overview(&map).into_iter().map(|r| r.0).collect::<Vec<_>>().join("\n");
        assert!(!text.contains("Quit"), "an unbound key is gone from the list");
        assert!(text.contains("Ctrl-J") && text.contains("goto"), "{text}");
        assert!(!text.contains("Delete line"), "ctrl-k now means something else");
    }

    /// Nothing may be handled outside the table: try a grid of key events on a fresh editor; whatever changes the editor
    /// must be a bound key (or a character that gets typed).
    #[test]
    fn no_key_does_anything_that_the_table_does_not_list() {
        let codes: Vec<KeyCode> = ('a'..='z').chain('0'..='9').chain("[]{}()'\"`;,./\\-=+_<>?!@#$%^&*~ ".chars()).map(KeyCode::Char)
            .chain((1..=12).map(KeyCode::F))
            .chain([KeyCode::Left, KeyCode::Right, KeyCode::Up, KeyCode::Down, KeyCode::Home, KeyCode::End, KeyCode::PageUp, KeyCode::PageDown, KeyCode::Tab, KeyCode::BackTab,
                    KeyCode::Esc, KeyCode::Enter, KeyCode::Backspace, KeyCode::Delete, KeyCode::Insert]).collect();
        let mods = [KeyModifiers::NONE, KeyModifiers::CONTROL, KeyModifiers::ALT, KeyModifiers::SHIFT, KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                    KeyModifiers::CONTROL | KeyModifiers::ALT, KeyModifiers::ALT | KeyModifiers::SHIFT];
        let map = build(&HashMap::new());
        let sig = |a: &mut App| {
            let b = a.buf();
            (b.contents(), b.cursors.iter().map(|c| (c.head, c.anchor)).collect::<Vec<_>>(), b.syntax)
        };
        let mut checked = 0;
        for &code in &codes { for &m in &mods {
            let ev = KeyEvent::new(code, m);
            let mut a = App::blank();
            a.buf().insert("ab cd\nef gh\nij kl");
            a.buf().cursors = vec![crate::buffer::Cursor::at(7)];
            let before = (sig(&mut a), a.tabs.len(), a.active, a.prompt.is_some(), a.picker.is_some(), a.menu, a.quit, a.line_numbers);
            a.on_key(ev);
            let after = (sig(&mut a), a.tabs.len(), a.active, a.prompt.is_some(), a.picker.is_some(), a.menu, a.quit, a.line_numbers);
            let spec = crate::app::key_spec(&ev);
            let printable = matches!(code, KeyCode::Char(_)) && !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            let shift_motion = m.contains(KeyModifiers::SHIFT) && map.get(&spec.replace("shift-", "")).map_or(false, |x| x.is_motion());
            if before != after && !printable && !shift_motion {
                assert!(map.contains_key(&spec), "{spec} changes the editor but is not in the key table");
            }
            checked += 1;
        } }
        assert!(checked > 600);
    }

    #[test]
    fn menu_labels_and_hints_come_from_the_table() {
        let map = build(&HashMap::new());
        assert_eq!(key_of(&map, Palette).as_deref(), Some("Ctrl-P"));
        assert_eq!(key_of(&map, Redo).as_deref(), Some("Ctrl-Y"));
        assert_eq!(key_of(&map, Unindent).as_deref(), Some("Shift-Tab"));
        assert_eq!(key_of(&map, NextTab).as_deref(), Some("Ctrl-PgDn"));
        assert_eq!(label("f10"), "F10");
        let h = hints(&Ctx::Edit { cursors: 1, selected: false }, &map);
        assert_eq!(h[0], ("F1".to_string(), "all keys".to_string()));
        let mut user = HashMap::new();
        user.insert("ctrl-s".to_string(), "none".to_string());
        user.insert("ctrl-b".to_string(), "save".to_string());
        let h = hints(&Ctx::Edit { cursors: 1, selected: false }, &build(&user));
        assert!(h.iter().any(|(k, d)| k == "Ctrl-B" && d == "save"), "a rebound key shows up in the hint line: {h:?}");
    }

    #[test]
    fn hints_follow_the_mode() {
        let map = build(&HashMap::new());
        let keys = |c: Ctx| hints(&c, &map).into_iter().map(|(k, d)| format!("{k} {d}")).collect::<Vec<_>>().join(", ");
        assert!(keys(Ctx::Edit { cursors: 1, selected: false }).contains("Ctrl-F find"));
        assert!(keys(Ctx::Edit { cursors: 1, selected: true }).contains("Ctrl-C copy"));
        let multi = keys(Ctx::Edit { cursors: 3, selected: false });
        assert!(multi.contains("Esc one cursor") && multi.contains("Alt-Up cursor up") && multi.contains("edits all 3"), "{multi}");
        assert!(keys(Ctx::Find).contains("F3 next"));
        assert!(keys(Ctx::Picker { keys: false }).contains("Enter select"));
        assert!(keys(Ctx::Menu).contains("Left/Right"));
    }
}
