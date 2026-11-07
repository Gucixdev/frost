//! frost - a small terminal editor. Fresh-like layout, micro-like keys, CUA + mouse + multi-cursor, Notepad++-style sessions.
mod app;
mod buffer;
mod config;
mod dict;
mod diff;
mod highlight;
mod session;
mod ui;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("frost [FILE...]\n  no files: reopen the last session (open tabs, unsaved text, cursors)\n  F1 keys, F10 menu, Ctrl-P command palette, Ctrl-Q quit (the session is saved on quit)");
        return Ok(());
    }
    let mut app = app::App::new(&args);

    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    // restore the terminal even when something panics
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        hook(p);
    }));
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    let mut last_save = std::time::Instant::now();
    let result = (|| -> io::Result<()> {
        while !app.quit {
            term.draw(|f| ui::draw(f, &mut app))?;
            let busy = app.tick();
            if event::poll(Duration::from_millis(if busy { 40 } else { 500 }))? {
                match event::read()? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
                    Event::Paste(s) => app.buf().paste(&s),
                    _ => {}
                }
                // drain what is already waiting (fast typing, pastes) before drawing again
                while event::poll(Duration::from_millis(0))? {
                    match event::read()? {
                        Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                        Event::Mouse(m) => app.on_mouse(m),
                        Event::Paste(s) => app.buf().paste(&s),
                        _ => {}
                    }
                }
            }
            // the session is also kept while running, so a crash or a killed terminal loses at most 30 s
            if last_save.elapsed() > Duration::from_secs(30) { app.save_session(); last_save = std::time::Instant::now(); }
        }
        Ok(())
    })();

    app.save_session();
    disable_raw_mode()?;
    execute!(term.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    term.show_cursor()?;
    result
}
