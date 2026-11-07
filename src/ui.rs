//! Drawing. Layout like Fresh: menu bar, tab bar, text with gutter, status bar, popups on top.
use crate::app::{display_col, App, MENUS, HELP};
use ratatui::layout::{Constraint, Layout as RLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

const BG: Color = Color::Rgb(30, 31, 36);
const BAR: Color = Color::Rgb(44, 46, 54);
const DIM: Color = Color::Rgb(120, 124, 138);
const FG: Color = Color::Rgb(205, 209, 220);
const ACCENT: Color = Color::Rgb(97, 175, 239);
const SEL: Color = Color::Rgb(62, 78, 110);
const CUR2: Color = Color::Rgb(229, 192, 123);

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG).fg(FG)), area);
    let rows = RLayout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).split(area);
    app.layout.menus.clear();
    app.layout.tabs.clear();
    app.layout.dropdown = None;
    app.layout.picker = None;

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
    if app.show_help { help(f, area); }
}

fn menu_bar(f: &mut Frame, app: &mut App, r: Rect) {
    f.render_widget(Block::default().style(Style::default().bg(BAR)), r);
    let mut x = r.x + 1;
    for (i, (name, _)) in MENUS.iter().enumerate() {
        let w = name.len() as u16 + 2;
        let rect = Rect::new(x, r.y, w, 1);
        let st = if app.menu == Some(i) { Style::default().bg(ACCENT).fg(BG) } else { Style::default().bg(BAR).fg(FG) };
        f.render_widget(Paragraph::new(format!(" {name} ")).style(st), rect);
        app.layout.menus.push((rect, i));
        x += w;
    }
}

fn tab_bar(f: &mut Frame, app: &mut App, r: Rect) {
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(36, 38, 45))), r);
    let mut x = r.x;
    for (i, b) in app.tabs.iter().enumerate() {
        let label = format!(" {}{} ", b.name(), if b.modified { " ●" } else { "" });
        let w = label.chars().count() as u16;
        if x + w > r.x + r.width { break; }
        let rect = Rect::new(x, r.y, w, 1);
        let st = if i == app.active { Style::default().bg(BG).fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().bg(Color::Rgb(36, 38, 45)).fg(DIM) };
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
            let st = Style::default().fg(if cur_line { ACCENT } else { DIM });
            row.push(Span::styled(format!("{:>w$} ", ln + 1, w = gutter as usize - 1), st));
        }
        let cs = cells(spans);
        let mut shown = 0;
        for (ch, st, col) in cs.iter().skip(hscroll) {
            if shown >= width { break; }
            let abs = start + col;
            let mut st = *st;
            if b.cursors.iter().any(|c| c.has_sel() && abs >= c.start() && abs < c.end()) { st = st.bg(SEL); }
            if b.cursors.iter().any(|c| c.head == abs && c.head != primary.head) { st = st.bg(CUR2).fg(BG); }
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
                if end_disp - hscroll == shown { row.push(Span::styled(" ", Style::default().bg(CUR2))); shown += 1; }
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
            f.set_cursor_position((r.x + gutter + (d - hscroll) as u16, r.y + (l - scroll) as u16));
        }
    }
}

fn status_bar(f: &mut Frame, app: &mut App, r: Rect) {
    f.render_widget(Block::default().style(Style::default().bg(BAR)), r);
    if let Some(p) = &app.prompt {
        let txt = format!(" {} {}", p.label, p.input);
        f.render_widget(Paragraph::new(txt.clone()).style(Style::default().bg(BAR).fg(FG)), r);
        f.set_cursor_position((r.x + txt.chars().count() as u16, r.y));
        return;
    }
    let i = app.active.min(app.tabs.len() - 1);
    let syn = app.hl.name_of(app.tabs[i].syntax);
    let b = &app.tabs[i];
    let (l, c) = b.pos(b.primary().head);
    let right = format!("{}  Ln {}, Col {}  {}  UTF-8  {}  ", if b.cursors.len() > 1 { format!("{} cursors", b.cursors.len()) } else { String::new() }, l + 1, c + 1, syn, if b.crlf { "CRLF" } else { "LF" });
    let left = if app.msg.is_empty() { format!(" {}{}  (F1 keys, F10 menu)", b.name(), if b.modified { " [modified]" } else { "" }) } else { format!(" {}", app.msg) };
    let w = r.width as usize;
    let pad = w.saturating_sub(left.chars().count() + right.chars().count());
    f.render_widget(Paragraph::new(format!("{left}{}{right}", " ".repeat(pad))).style(Style::default().bg(BAR).fg(FG)), r);
}

fn dropdown(f: &mut Frame, app: &mut App, mi: usize, bar: Rect) {
    let items = MENUS[mi].1;
    let x = app.layout.menus.iter().find(|(_, i)| *i == mi).map(|(r, _)| r.x).unwrap_or(bar.x);
    let w = items.iter().map(|it| it.0.len() + it.1.len() + 4).max().unwrap_or(10) as u16 + 2;
    let rect = Rect::new(x, bar.y + 1, w.min(f.area().width.saturating_sub(x)), items.len() as u16 + 1);
    f.render_widget(Clear, rect);
    let mut lines = vec![Line::from("")];
    lines.clear();
    for (n, it) in items.iter().enumerate() {
        let pad = (w as usize).saturating_sub(it.0.len() + it.1.len() + 3);
        let hov = app.hover_row == Some(n);
        let row_style = if hov { Style::default().bg(ACCENT).fg(BG) } else { Style::default() };
        lines.push(Line::from(vec![Span::raw(format!(" {}", it.0)), Span::raw(" ".repeat(pad)), Span::styled(it.1.to_string(), if hov { row_style } else { Style::default().fg(DIM) }), Span::raw(" ")]).style(row_style));
    }
    // row 0 of the rect is a thin top border line; items start at row 1 (the mouse code relies on this)
    let top = Rect::new(rect.x, rect.y, rect.width, 1);
    f.render_widget(Paragraph::new("").style(Style::default().bg(BAR)), top);
    let body = Rect::new(rect.x, rect.y + 1, rect.width, rect.height - 1);
    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BAR).fg(FG)), body);
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
    let h = (shown.len() as u16 + 3).clamp(5, 20);
    let rect = centered(area, 56, h);
    f.render_widget(Clear, rect);
    let title = match pk.kind { crate::app::PickerKind::Palette => " Command palette ", crate::app::PickerKind::Language => " Language ", crate::app::PickerKind::Sessions => " Sessions ", crate::app::PickerKind::Closed => " Closed tabs " };
    let block = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(ACCENT)).style(Style::default().bg(BAR).fg(FG));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = inner.height.saturating_sub(1) as usize;
    let first = pk.sel.saturating_sub(rows.saturating_sub(1));
    let mut lines = vec![Line::from(vec![Span::styled("> ", Style::default().fg(ACCENT)), Span::raw(pk.filter.clone())])];
    for (n, &i) in shown.iter().enumerate().skip(first).take(rows) {
        let st = if n == pk.sel { Style::default().bg(ACCENT).fg(BG) } else { Style::default() };
        // Language list is alphabetical: show the initial letter in a margin where a new letter starts
        let lang = matches!(pk.kind, crate::app::PickerKind::Language) && pk.filter.is_empty();
        let ini = |s: &str| s.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or(' ');
        let mark = if lang && (n == 0 || ini(&pk.items[shown[n - 1]]) != ini(&pk.items[i])) { ini(&pk.items[i]) } else { ' ' };
        lines.push(Line::styled(format!(" {} {}", mark, pk.items[i]), st));
    }
    f.render_widget(Paragraph::new(lines), inner);
    f.set_cursor_position((inner.x + 2 + pk.filter.chars().count() as u16, inner.y));
    app.layout.picker = Some(rect);
}

fn help(f: &mut Frame, area: Rect) {
    let rect = centered(area, 70, 12);
    f.render_widget(Clear, rect);
    let block = Block::default().borders(Borders::ALL).title(" Keys (any key closes) ").border_style(Style::default().fg(ACCENT)).style(Style::default().bg(BAR).fg(FG));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(Paragraph::new(HELP.replace(" | ", "\n\n")).wrap(Wrap { trim: true }), inner);
}
