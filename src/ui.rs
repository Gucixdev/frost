//! Drawing. Layout like Fresh: menu bar, tab bar, text with gutter, status bar, popups on top.
use crate::app::{display_col, App, MENUS};
use ratatui::layout::{Constraint, Layout as RLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;


pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(crate::config::theme().bg).fg(crate::config::theme().fg)), area);
    let rows = RLayout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).split(area);
    app.layout.menus.clear();
    app.layout.tabs.clear();
    app.layout.dropdown = None;
    app.layout.picker = None;
    app.layout.caret = None;

    menu_bar(f, app, rows[0]);
    tab_bar(f, app, rows[1]);

    let gutter = if app.line_numbers { (app.tabs[app.active.min(app.tabs.len() - 1)].lines().to_string().len() + 2) as u16 } else { 0 };
    app.layout.gutter = gutter;
    app.layout.text = rows[2];
    app.scroll_to_cursor();
    text_area(f, app, rows[2], gutter);
    status_bar(f, app, rows[3]);

    if let Some(mi) = app.menu { dropdown(f, app, mi, rows[0]); }
    if app.picker.is_some() { picker(f, app, area); }
}

fn menu_bar(f: &mut Frame, app: &mut App, r: Rect) {
    f.render_widget(Block::default().style(Style::default().bg(crate::config::theme().bar)), r);
    let mut x = r.x + 1;
    for (i, (name, _)) in MENUS.iter().enumerate() {
        let w = name.len() as u16 + 2;
        if x + w > r.right() { break; }   // a narrow terminal shows only the menus that fit
        let rect = Rect::new(x, r.y, w, 1);
        let st = if app.menu == Some(i) { crate::config::accent_style() } else { Style::default().bg(crate::config::theme().bar).fg(crate::config::theme().fg) };
        f.render_widget(Paragraph::new(format!(" {name} ")).style(st), rect);
        app.layout.menus.push((rect, i));
        x += w;
    }
}

fn tab_bar(f: &mut Frame, app: &mut App, r: Rect) {
    let t = crate::config::theme();
    // the bar, the idle tabs and the active tab must all differ, in every theme (the terminal theme has no RGB to mix)
    let idle_fg = if t.terminal { Color::Gray } else { t.fg };
    f.render_widget(Block::default().style(Style::default().bg(t.bar)), r);
    let mut x = r.x;
    for (i, b) in app.tabs.iter().enumerate() {
        let label = format!(" {}{} ", b.name(), if b.modified { " ●" } else { "" });
        let w = label.chars().count() as u16;
        if x + w > r.x + r.width { break; }
        let rect = Rect::new(x, r.y, w, 1);
        let st = if i == app.active { Style::default().bg(t.bg).fg(t.accent).add_modifier(Modifier::BOLD) } else { Style::default().bg(t.bar).fg(idle_fg) };
        f.render_widget(Paragraph::new(label).style(st), rect);
        app.layout.tabs.push((rect, i));
        x += w + 1;
    }
}

/// one screen line: (char, style, char column in the buffer line); a tab becomes several cells
fn cells(spans: &[(Style, String)]) -> Vec<(char, Style, usize)> {
    let mut out = vec![];
    let (mut col, mut disp) = (0usize, 0usize);
    for (st, s) in spans {
        for ch in s.chars() {
            if ch == '\t' {
                let n = 4 - disp % 4;
                for _ in 0..n { out.push((' ', *st, col)); }
                disp += n;
            } else {
                out.push((ch, *st, col));
                disp += 1;
            }
            col += 1;
        }
    }
    out
}

fn text_area(f: &mut Frame, app: &mut App, r: Rect, gutter: u16) {
    let i = app.active.min(app.tabs.len() - 1);
    let (scroll, hscroll, syntax) = (app.tabs[i].scroll, app.tabs[i].hscroll, app.tabs[i].syntax);
    let total = app.tabs[i].lines();
    let count = (r.height as usize).min(total.saturating_sub(scroll));
    // the highlighter keeps parser checkpoints; tell it where the text last changed
    {
        let b = &mut app.tabs[i];
        let key = (b.id, b.version);
        if app.hl_key != key {
            // another tab, or the text changed: parser checkpoints from the first changed line on are stale
            app.hl.invalidate_from(if app.hl_key.0 == b.id { b.first_changed } else { 0 });
            app.hl_key = key;
            b.first_changed = usize::MAX;
        }
    }
    let b = &app.tabs[i];
    let styled = app.hl.styled(syntax, total, &|n| b.line_string(n), scroll, count);
    let primary = b.primary();
    let width = r.width.saturating_sub(gutter) as usize;
    let mut lines: Vec<Line> = vec![];
    for (k, spans) in styled.iter().enumerate() {
        let ln = scroll + k;
        let start = b.text.line_to_char(ln);
        let mut row: Vec<Span> = vec![];
        if gutter > 0 {
            let cur_line = b.cursors.iter().any(|c| b.pos(c.head).0 == ln);
            let st = Style::default().fg(if cur_line { crate::config::theme().accent } else { crate::config::theme().dim });
            row.push(Span::styled(format!("{:>w$} ", ln + 1, w = gutter as usize - 1), st));
        }
        let cs = cells(spans);
        let mut shown = 0;
        for (ch, st, col) in cs.iter().skip(hscroll) {
            if shown >= width { break; }
            let abs = start + col;
            let mut st = *st;
            if b.cursors.iter().any(|c| c.has_sel() && abs >= c.start() && abs < c.end()) { st = st.bg(crate::config::theme().sel); }
            if b.cursors.iter().any(|c| c.head == abs && c.head != primary.head) { st = st.patch(crate::config::caret2_style()); }
            row.push(Span::styled(ch.to_string(), st));
            shown += 1;
        }
        // a secondary caret at the end of its line has no character under it: draw a block
        let line_len = b.line_len(ln);
        let end_disp = display_col(&b.line_string(ln), line_len);
        for c in b.cursors.iter().filter(|c| c.head != primary.head) {
            if c.head == start + line_len && end_disp >= hscroll && end_disp - hscroll < width {
                let pad = (end_disp - hscroll).saturating_sub(shown);
                if pad > 0 { row.push(Span::raw(" ".repeat(pad))); shown += pad; }
                if end_disp - hscroll == shown { row.push(Span::styled(" ", crate::config::caret2_style())); shown += 1; }
            }
        }
        lines.push(Line::from(row));
    }
    f.render_widget(Paragraph::new(lines), r);
    // the real terminal caret for the primary cursor
    let (l, c) = b.pos(primary.head);
    if l >= scroll && l < scroll + r.height as usize && app.prompt.is_none() && app.picker.is_none() {
        let d = display_col(&b.line_string(l), c);
        if d >= hscroll && d - hscroll < width {
            let pos = (r.x + gutter + (d - hscroll) as u16, r.y + (l - scroll) as u16);
            app.layout.caret = Some(pos);
            f.set_cursor_position(pos);
        }
    }
}

/// "Ctrl-S save  Ctrl-F find ...": key in bold, name dim, as many as fit in `room` cells
fn hint_spans(h: &[(String, String)], room: usize) -> Vec<Span<'static>> {
    let t = crate::config::theme();
    let mut out = vec![];
    let mut used = 0;
    for (k, d) in h {
        let w = 1 + k.chars().count() + 1 + d.chars().count() + 1;
        if used + w > room { break; }
        used += w;
        out.push(Span::styled(format!(" {k}"), Style::default().fg(t.fg).add_modifier(Modifier::BOLD)));
        out.push(Span::styled(format!(" {d} "), Style::default().fg(if t.terminal { Color::Gray } else { t.dim })));
    }
    out
}

fn status_bar(f: &mut Frame, app: &mut App, r: Rect) {
    let t = crate::config::theme();
    let bar = Style::default().bg(t.bar).fg(t.fg);
    f.render_widget(Block::default().style(bar), r);
    let w = r.width as usize;
    let hints = { let ctx = app.hint_ctx(); crate::keys::hints(&ctx, &app.keymap) };
    if let Some(p) = &app.prompt {
        let txt = format!(" {} {}", p.label, p.input);
        let n = txt.chars().count();
        // the hints go to the right of what is typed and give way when the line gets long
        let room = w.saturating_sub(n + 2);
        let spans = hint_spans(&hints, room);
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let mut row = vec![Span::raw(txt.clone()), Span::raw(" ".repeat(w.saturating_sub(n + used)))];
        row.extend(spans);
        f.render_widget(Paragraph::new(Line::from(row)).style(bar), r);
        f.set_cursor_position((r.x + (n as u16).min(r.width.saturating_sub(1)), r.y));
        return;
    }
    let i = app.active.min(app.tabs.len() - 1);
    let syn = app.hl.name_of(app.tabs[i].syntax);
    let msg = app.msg_live().map(|m| m.to_string());
    let b = &app.tabs[i];
    let (l, c) = b.pos(b.primary().head);
    let sel: usize = b.cursors.iter().map(|c| c.end() - c.start()).sum();
    let right = format!("{}{}Ln {}, Col {}  {}  {}{} ",
        if b.cursors.len() > 1 { format!("{} cursors  ", b.cursors.len()) } else { String::new() },
        if sel > 0 { format!("{sel} selected  ") } else { String::new() },
        l + 1, c + 1, syn, if b.crlf { "CRLF" } else { "LF" }, b.load_pct().map(|p| format!("  loading {p}%")).unwrap_or_default());
    // the message never pushes the right part off the screen: it is cut instead
    let room = w.saturating_sub(right.chars().count());
    let mut row: Vec<Span> = match &msg {
        Some(m) => {
            let left = format!(" {m}");
            let left: String = if left.chars().count() > room { left.chars().take(room.saturating_sub(1)).chain(std::iter::once('~')).collect() } else { left };
            vec![Span::styled(left, bar.add_modifier(Modifier::BOLD))]
        }
        None => hint_spans(&hints, room.saturating_sub(1)),
    };
    let used: usize = row.iter().map(|s| s.content.chars().count()).sum();
    row.push(Span::raw(" ".repeat(w.saturating_sub(used + right.chars().count()))));
    row.push(Span::raw(right));
    f.render_widget(Paragraph::new(Line::from(row)).style(bar), r);
}

fn dropdown(f: &mut Frame, app: &mut App, mi: usize, bar: Rect) {
    let items = MENUS[mi].1;
    // the shortcut shown is whatever the key table binds to the command right now
    let keys: Vec<String> = items.iter().map(|it| crate::keys::key_of(&app.keymap, it.1).unwrap_or_default()).collect();
    let x = app.layout.menus.iter().find(|(_, i)| *i == mi).map(|(r, _)| r.x).unwrap_or(bar.x);
    let w = items.iter().zip(&keys).map(|(it, k)| it.0.len() + k.len() + 4).max().unwrap_or(10) as u16 + 2;
    let rect = Rect::new(x, bar.y + 1, w.min(f.area().width.saturating_sub(x)), items.len() as u16 + 1);
    f.render_widget(Clear, rect);
    let mut lines = vec![];
    for (n, (it, k)) in items.iter().zip(&keys).enumerate() {
        let pad = (w as usize).saturating_sub(it.0.len() + k.len() + 3);
        let hov = app.hover_row == Some(n);
        let row_style = if hov { crate::config::accent_style() } else { Style::default() };
        lines.push(Line::from(vec![Span::raw(format!(" {}", it.0)), Span::raw(" ".repeat(pad)), Span::styled(k.clone(), if hov { row_style } else { Style::default().fg(crate::config::theme().dim) }), Span::raw(" ")]).style(row_style));
    }
    // row 0 of the rect is a thin top border line; items start at row 1 (the mouse code relies on this)
    let top = Rect::new(rect.x, rect.y, rect.width, 1);
    f.render_widget(Paragraph::new("").style(Style::default().bg(crate::config::theme().bar)), top);
    let body = Rect::new(rect.x, rect.y + 1, rect.width, rect.height - 1);
    f.render_widget(Paragraph::new(lines).style(Style::default().bg(crate::config::theme().bar).fg(crate::config::theme().fg)), body);
    app.layout.dropdown = Some((rect, mi));
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 4, w, h)
}

fn picker(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(pk) = app.picker.as_ref() else { return };
    let shown = pk.shown();
    let h = (shown.len() as u16 + 3).clamp(5, area.height.saturating_sub(4).clamp(5, 30));
    let rect = centered(area, if pk.kind == crate::app::PickerKind::Keys { 68 } else { 56 }, h);
    f.render_widget(Clear, rect);
    let title = match pk.kind { crate::app::PickerKind::Palette => " Command palette ", crate::app::PickerKind::Language => " Language ", crate::app::PickerKind::Sessions => " Sessions ", crate::app::PickerKind::Closed => " Closed tabs ", crate::app::PickerKind::DiffTab => " Compare with ", crate::app::PickerKind::Word => " Jump to word (buffer + dictionary) ", crate::app::PickerKind::Keys => " Keys (type to filter, Enter runs) " };
    let block = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(crate::config::theme().accent)).style(Style::default().bg(crate::config::theme().bar).fg(crate::config::theme().fg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = inner.height.saturating_sub(1) as usize;
    let first = pk.sel.saturating_sub(rows.saturating_sub(1));
    let mut lines = vec![Line::from(vec![Span::styled("> ", Style::default().fg(crate::config::theme().accent)), Span::raw(pk.filter.clone())])];
    for (n, &i) in shown.iter().enumerate().skip(first).take(rows) {
        let st = if n == pk.sel { crate::config::accent_style() } else { Style::default() };
        // Language list is alphabetical: show the initial letter in a margin where a new letter starts
        let lang = matches!(pk.kind, crate::app::PickerKind::Language) && pk.filter.is_empty();
        let ini = |s: &str| s.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or(' ');
        let mark = if lang && (n == 0 || ini(&pk.items[shown[n - 1]]) != ini(&pk.items[i])) { ini(&pk.items[i]) } else { ' ' };
        // group headings of the key list
        let st = if pk.items[i].starts_with("-- ") && n != pk.sel { Style::default().fg(crate::config::theme().dim).add_modifier(Modifier::BOLD) } else { st };
        lines.push(Line::styled(format!(" {} {}", mark, pk.items[i]), st));
    }
    f.render_widget(Paragraph::new(lines), inner);
    f.set_cursor_position((inner.x + 2 + pk.filter.chars().count() as u16, inner.y));
    app.layout.picker = Some(rect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};

    fn rows(t: &Terminal<TestBackend>) -> Vec<String> {
        let b = t.backend().buffer();
        (0..b.area.height).map(|y| (0..b.area.width).map(|x| b[(x, y)].symbol().to_string()).collect()).collect()
    }

    #[test]
    fn a_message_lives_in_the_status_bar_and_never_covers_the_text() {
        let mut app = App::blank();
        app.buf().insert("line one\nline two\nline three");
        let mut t = Terminal::new(TestBackend::new(70, 10)).unwrap();
        t.draw(|f| draw(f, &mut app)).unwrap();
        let before = rows(&t);
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        t.draw(|f| draw(f, &mut app)).unwrap();
        let after = rows(&t);
        let last = after.len() - 1;
        assert!(after[last].contains("copied 3 lines"), "{:?}", after[last]);
        assert!(after[last].contains("Ln 3"), "the cursor position stays visible: {:?}", after[last]);
        assert_eq!(&before[2..last], &after[2..last], "no text row changed");
    }

    #[test]
    fn a_long_message_is_cut_not_the_position() {
        let mut app = App::blank();
        app.msg = "x".repeat(200);
        let mut t = Terminal::new(TestBackend::new(60, 6)).unwrap();
        t.draw(|f| draw(f, &mut app)).unwrap();
        let last = rows(&t).pop().unwrap();
        assert!(last.contains("Ln 1, Col 1") && last.contains('~'), "{last:?}");
    }

    fn key(c: char, m: KeyModifiers) -> KeyEvent { KeyEvent::new(KeyCode::Char(c), m) }
    fn last_row(app: &mut App, w: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, 12)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        rows(&t).pop().unwrap()
    }

    #[test]
    fn the_hint_line_follows_what_you_are_doing() {
        let mut app = App::blank();
        app.buf().insert("foo bar foo");
        let idle = last_row(&mut app, 100);
        assert!(idle.contains("F1 all keys") && idle.contains("Ctrl-S save") && idle.contains("Ctrl-F find"), "{idle:?}");
        app.on_key(key('d', KeyModifiers::CONTROL));   // selects the word
        let sel = last_row(&mut app, 100);
        assert!(sel.contains("Ctrl-C copy") && sel.contains("Ctrl-D next match"), "{sel:?}");
        app.on_key(key('d', KeyModifiers::CONTROL));
        let multi = last_row(&mut app, 100);
        assert!(multi.contains("Esc one cursor") && multi.contains("2 cursors") && multi.contains("edits all 2"), "{multi:?}");
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        app.on_key(key('f', KeyModifiers::CONTROL));
        let find = last_row(&mut app, 100);
        assert!(find.contains("Find:") && find.contains("F3 next") && find.contains("Esc cancel"), "{find:?}");
        // a narrow terminal drops hints from the end instead of overflowing
        let narrow = last_row(&mut app, 40);
        assert!(narrow.contains("Find:") && narrow.chars().count() == 40, "{narrow:?}");
    }

    #[test]
    fn f1_lists_the_keys_and_enter_runs_the_command() {
        let mut app = App::blank();
        app.on_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        let mut t = Terminal::new(TestBackend::new(80, 30)).unwrap();
        t.draw(|f| draw(f, &mut app)).unwrap();
        let screen = rows(&t).join("\n");
        assert!(screen.contains("Keys") && screen.contains("Ctrl-S") && screen.contains("Save") && screen.contains("-- File"), "{screen}");
        // typing filters the list; Enter on a command runs it
        for c in "palette".chars() { app.on_key(key(c, KeyModifiers::NONE)); }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.picker.as_ref().map_or(false, |p| p.kind == crate::app::PickerKind::Palette), "the palette opened");
    }

    #[test]
    fn menu_shortcuts_are_the_table_keys() {
        let mut app = App::blank();
        app.menu = Some(1);   // Edit
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, &mut app)).unwrap();
        let screen = rows(&t).join("\n");
        assert!(screen.contains("Redo") && screen.contains("Ctrl-Y") && screen.contains("Unindent") && screen.contains("Shift-Tab"), "{screen}");
        app.keymap.insert("ctrl-b".into(), crate::app::Action::Redo);
        app.keymap.remove("ctrl-y");
        app.keymap.remove("ctrl-shift-z");
        t.draw(|f| draw(f, &mut app)).unwrap();
        assert!(rows(&t).join("\n").contains("Ctrl-B"), "a rebound key shows in the menu");
    }
}
