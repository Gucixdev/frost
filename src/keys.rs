//! The one keymap table. Key dispatch, the menu shortcut labels, the hint line and the F1 panel are all generated from
//! `KEYS` (every action has a row, bound or not), so none of them can disagree with what a key really does. Two things edit the
//! effective map on top of the table: `frost.keys["ctrl-j"] = "goto"` in init.lua (`"none"` unbinds), and the F1 panel, which
//! saves its changes as the same kind of lines in keys.lua (next to init.lua, loaded after it).
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
    b("", DiffDisk, "", "Diff with the file on disk", "File"),
    b("", DiffTab, "", "Diff with another tab...", "File"),
    b("", ClosedTabs, "", "List the closed tabs...", "File"),
    b("", SaveSession, "", "Save the session under a name...", "File"),
    b("", LoadSession, "", "Load a named session...", "File"),

    b("ctrl-pageup alt-,", PrevTab, "prev tab", "Previous tab", "Tabs"),
    b("ctrl-pagedown alt-.", NextTab, "next tab", "Next tab", "Tabs"),
    b("alt-1", Tab1, "", "Go to tab 1", "Tabs"),
    b("alt-2", Tab2, "", "Go to tab 2", "Tabs"),
    b("alt-3", Tab3, "", "Go to tab 3", "Tabs"),
    b("alt-4", Tab4, "", "Go to tab 4", "Tabs"),
    b("alt-5", Tab5, "", "Go to tab 5", "Tabs"),
    b("alt-6", Tab6, "", "Go to tab 6", "Tabs"),
    b("alt-7", Tab7, "", "Go to tab 7", "Tabs"),
    b("alt-8", Tab8, "", "Go to tab 8", "Tabs"),
    b("alt-9", Tab9, "", "Go to tab 9", "Tabs"),
    b("alt-0", TabLast, "", "Go to the last tab", "Tabs"),

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
    b("", ToggleLineNumbers, "", "Show or hide the line numbers", "View"),
    b("f1", Help, "all keys", "Keys: this list, where you change them", "View"),
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

/// "ctrl-shift-s" -> "Ctrl-Shift-S", "pagedown" -> "PgDn", "shift-backtab" -> "Shift-Tab", "alt-," -> "Alt-,"
pub fn label(spec: &str) -> String {
    let mut rest = spec;
    let mut out = String::new();
    for (p, l) in [("ctrl-", "Ctrl-"), ("alt-", "Alt-"), ("shift-", "Shift-")] { if let Some(r) = rest.strip_prefix(p) { out.push_str(l); rest = r; } }
    out.push_str(&match rest {
        "pageup" => "PgUp".to_string(), "pagedown" => "PgDn".to_string(), "backtab" => "Tab".to_string(),
        p if p.len() > 1 && p.starts_with('f') && p[1..].chars().all(|c| c.is_ascii_digit()) => p.to_uppercase(),
        p => { let mut c = p.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default() }
    });
    out
}

/// the first key bound to `a` (table order), for menus and hints
pub fn key_of(m: &HashMap<String, Action>, a: Action) -> Option<String> {
    let rank = |k: &str| KEYS.iter().enumerate().find_map(|(i, r)| r.keys.split_whitespace().position(|x| x == k).map(|j| (i, j))).unwrap_or((usize::MAX, 0));
    m.iter().filter(|(_, x)| **x == a).map(|(k, _)| k).min_by_key(|k| (rank(k), k.len(), (*k).clone())).map(|k| label(k))
}

/// the keys bound to `a` right now: the table's own order first, keys added later after them
pub fn keys_of(m: &HashMap<String, Action>, a: Action) -> Vec<String> {
    let mut v: Vec<String> = KEYS.iter().filter(|r| r.act == a).flat_map(|r| r.keys.split_whitespace()).filter(|k| m.get(*k) == Some(&a)).map(String::from).collect();
    let mut extra: Vec<String> = m.iter().filter(|(k, x)| **x == a && !v.contains(k)).map(|(k, _)| k.clone()).collect();
    extra.sort();
    v.extend(extra);
    v
}

pub fn desc(a: Action) -> &'static str { KEYS.iter().find(|r| r.act == a).map_or("", |r| r.desc) }

/// the name init.lua and keys.lua use for an action ("save", "reopenclosed", "tab3")
pub fn name(a: Action) -> String { format!("{a:?}").to_lowercase() }

/// the F1 panel: (text, action) rows, a group heading before each group. Every action of the table has a row, with its
/// keys as bound right now or "(none)".
pub fn overview(m: &HashMap<String, Action>) -> Vec<(String, Option<Action>)> {
    let mut out: Vec<(String, Option<Action>)> = vec![];
    let mut group = "";
    for r in KEYS {
        if r.group != group { group = r.group; out.push((format!("-- {group}"), None)); }
        let mine: Vec<String> = keys_of(m, r.act).iter().map(|k| label(k)).collect();
        let keys = if mine.is_empty() { "(none)".to_string() } else { mine.join(" / ") };
        out.push((format!("{keys:<22} {}", r.desc), Some(r.act)));
    }
    out
}

/// bind `key` to `a`. `add`: keep the action's other keys, otherwise this becomes its only key. A key can mean one thing, so the
/// action that had it loses it; that action is returned.
pub fn rebind(m: &mut HashMap<String, Action>, a: Action, key: &str, add: bool) -> Option<Action> {
    if !add { m.retain(|_, x| *x != a); }
    m.insert(key.to_string(), a).filter(|old| *old != a)
}

pub fn unbind(m: &mut HashMap<String, Action>, a: Action) { m.retain(|_, x| *x != a); }

/// back to the table's keys for `a`. A default key that another action holds now stays with that action; their number is returned.
pub fn reset(m: &mut HashMap<String, Action>, a: Action) -> usize {
    unbind(m, a);
    let mut kept = 0;
    for k in KEYS.iter().filter(|r| r.act == a).flat_map(|r| r.keys.split_whitespace()) {
        if m.contains_key(k) { kept += 1; } else { m.insert(k.to_string(), a); }
    }
    kept
}

/// what differs from the table, as (key, action name or "none"), sorted: exactly what keys.lua has to say
pub fn overrides(m: &HashMap<String, Action>) -> Vec<(String, String)> {
    let d = build(&HashMap::new());
    let mut v: Vec<(String, String)> = m.iter().filter(|(k, a)| d.get(*k) != Some(*a)).map(|(k, a)| (k.clone(), name(*a))).collect();
    v.extend(d.keys().filter(|k| !m.contains_key(*k)).map(|k| (k.clone(), "none".to_string())));
    v.sort();
    v
}

fn lua_quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() { match c { '\\' => o.push_str("\\\\"), '"' => o.push_str("\\\""), c if c.is_control() => o.push_str(&format!("\\{:03}", c as u32)), c => o.push(c) } }
    o.push('"');
    o
}

/// the text of keys.lua for `overrides`
pub fn keys_lua(over: &[(String, String)]) -> String {
    let mut s = String::from("-- written by frost's key panel (F1); loaded after init.lua, so it has the last word. Delete a line to get the default back.\n");
    for (k, a) in over { s.push_str(&format!("frost.keys[{}] = {}\n", lua_quote(k), lua_quote(a))); }
    s
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
        Ctx::Picker { keys: true } => lit(&[("type", "filter"), ("Enter", "rebind"), ("Ins", "add key"), ("Del", "unbind"), ("Ctrl-R", "reset"), ("Esc", "close")]),
        Ctx::Capture => lit(&[("a key", "binds it"), ("Esc", "cancel")]),
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
pub enum Ctx { Menu, Capture, Picker { keys: bool }, Find, Prompt, Edit { cursors: usize, selected: bool } }

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

    /// split "ctrl-alt-x" into its modifiers and the key (the key itself may be "-")
    fn parts(k: &str) -> (Vec<&str>, &str) {
        let mut rest = k;
        let mut mods = vec![];
        for m in ["ctrl", "alt", "shift"] { if let Some(r) = rest.strip_prefix(m).and_then(|r| r.strip_prefix('-')) { mods.push(m); rest = r; } }
        (mods, rest)
    }

    #[test]
    fn every_table_key_is_spelled_the_way_key_spec_prints_it() {
        // a typo in the table would be a key that can never fire
        for r in KEYS { for k in r.keys.split_whitespace() {
            let (mods, c) = parts(k);
            assert!(!mods.is_empty() || c.len() > 1, "{k}: a plain printable key would stop typing");
            let c1 = c.chars().count() == 1 && !c.chars().all(|ch| ch.is_ascii_uppercase());
            assert!(c1 || ["left", "right", "up", "down", "home", "end", "pageup", "pagedown", "tab", "backtab", "esc", "enter", "backspace", "delete"].contains(&c) || (c.starts_with('f') && c[1..].parse::<u8>().is_ok()), "{k}");
        } }
    }

    #[test]
    fn every_action_has_exactly_one_row_with_its_own_description() {
        let mut acts: Vec<Action> = vec![];
        let mut descs: Vec<&str> = vec![];
        for r in KEYS {
            assert!(!acts.contains(&r.act), "{:?} has two rows", r.act);
            assert!(!descs.contains(&r.desc), "two rows say '{}'", r.desc);
            acts.push(r.act);
            descs.push(r.desc);
        }
        // everything a menu or the palette can run is in the table, so it is in the panel too
        for a in crate::app::MENUS.iter().flat_map(|m| m.1.iter().map(|i| i.1)).chain(crate::app::palette_actions()) { assert!(acts.contains(&a), "{a:?} is runnable but has no row in the key table"); }
        assert!(["tab1", "tab9", "tablast", "nexttab", "prevtab"].iter().all(|n| crate::app::action_by_name(n).is_some()));
    }

    #[test]
    fn tab_keys_are_in_the_table_and_the_panel_lists_them() {
        let map = build(&HashMap::new());
        assert_eq!(map["alt-1"], Tab1);
        assert_eq!(map["alt-9"], Tab9);
        assert_eq!(map["alt-0"], TabLast);
        assert_eq!(map["alt-."], NextTab);
        assert_eq!(map["alt-,"], PrevTab);
        assert_eq!(map["ctrl-pagedown"], NextTab);
        let text = overview(&map).into_iter().map(|r| r.0).collect::<Vec<_>>().join("\n");
        for want in ["-- Tabs", "Alt-3 ", "Go to tab 3", "Ctrl-PgDn / Alt-.", "Alt-0 "] { assert!(text.contains(want), "{want}:\n{text}"); }
    }

    #[test]
    fn the_overview_shows_every_bound_key_and_every_action() {
        let mut user = HashMap::new();
        user.insert("ctrl-j".to_string(), "goto".to_string());     // an extra binding
        user.insert("ctrl-q".to_string(), "none".to_string());     // an unbound default
        user.insert("ctrl-k".to_string(), "save".to_string());     // a rebound default
        for map in [build(&HashMap::new()), build(&user)] {
            let rows = overview(&map);
            let text = rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>().join("\n");
            for (spec, act) in &map {
                let lab = label(spec);
                let row = text.lines().find(|l| l.split(" / ").any(|p| p.trim_start().starts_with(&lab) && { let rest = &p.trim_start()[lab.len()..]; rest.is_empty() || rest.starts_with(' ') }));
                assert!(row.is_some(), "{spec} ({act:?}) is bound but the overview does not show it:\n{text}");
            }
            for r in KEYS { assert!(rows.iter().any(|x| x.1 == Some(r.act)), "{:?} has no row", r.act); }
        }
        let map = build(&user);
        let text = overview(&map).into_iter().map(|r| r.0).collect::<Vec<_>>().join("\n");
        let row = |d: &str| text.lines().find(|l| l.contains(d)).unwrap().to_string();
        assert!(row("Quit (the session").starts_with("(none)"), "an unbound action says so: {}", row("Quit (the session"));
        assert!(row("Go to line").starts_with("Ctrl-G / Ctrl-J "), "{}", row("Go to line"));
        let save = text.lines().find(|l| l.ends_with(" Save")).unwrap();
        assert!(save.starts_with("Ctrl-S / Ctrl-K "), "{save}");
        assert!(!row("Delete line").contains("Ctrl-K"), "ctrl-k now means something else");
    }

    #[test]
    fn rebind_unbind_reset_keep_one_meaning_per_key() {
        let mut m = build(&HashMap::new());
        // replace: the action's only key now, and the previous owner loses it
        assert_eq!(rebind(&mut m, Save, "ctrl-j", false), None);
        assert_eq!(keys_of(&m, Save), vec!["ctrl-j"]);
        assert_eq!(rebind(&mut m, Goto, "ctrl-j", true), Some(Save));
        assert_eq!(keys_of(&m, Goto), vec!["ctrl-g", "ctrl-j"]);
        assert!(keys_of(&m, Save).is_empty());
        // the same key again is not a conflict
        assert_eq!(rebind(&mut m, Goto, "ctrl-j", true), None);
        unbind(&mut m, Goto);
        assert!(keys_of(&m, Goto).is_empty() && !m.contains_key("ctrl-g"));
        // reset puts the table's keys back, unless another action holds them
        m.insert("ctrl-g".to_string(), Find);
        assert_eq!(reset(&mut m, Goto), 1);
        assert_eq!(m["ctrl-g"], Find);
        assert_eq!(reset(&mut m, Save), 0);
        assert_eq!(keys_of(&m, Save), vec!["ctrl-s"]);
    }

    #[test]
    fn what_the_panel_saves_loads_back_to_the_same_map() {
        let mut m = build(&HashMap::new());
        assert!(overrides(&m).is_empty(), "the defaults need no file");
        rebind(&mut m, Save, "ctrl-j", false);
        rebind(&mut m, Tab2, "alt-\"", true);       // quote and backslash must survive the Lua text
        rebind(&mut m, Tab3, "alt-\\", true);
        rebind(&mut m, Tab4, "alt--", true);
        unbind(&mut m, Quit);
        let lua = keys_lua(&overrides(&m));
        let mut cfg = crate::config::test_config();
        crate::config::run_chunks(&["", &lua], &mut cfg).unwrap();
        assert_eq!(build(&cfg.keys), m, "{lua}");
        assert!(lua.contains("frost.keys[\"ctrl-j\"] = \"save\"") && lua.contains("\"ctrl-q\"] = \"none\""), "{lua}");
        assert_eq!(label("alt--"), "Alt--");
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
            let shift_motion = m.contains(KeyModifiers::SHIFT) && map.get(&spec.replace("shift-", "")).is_some_and(|x| x.is_motion());
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
