//! frost - a small terminal editor. Fresh-like layout, micro-like keys, CUA + mouse + multi-cursor, Notepad++-style sessions.
mod app;
mod buffer;
mod config;
mod dict;
mod diff;
mod highlight;
mod history_store;
mod session;
mod ui;

use crossterm::event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyEventKind};
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
    execute!(out, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    // restore the terminal even when something panics
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture, LeaveAlternateScreen);
        hook(p);
    }));
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    // a shutdown or a closed terminal (SIGTERM/SIGHUP) saves the session and the undo history like a normal quit
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP, signal_hook::consts::SIGINT] {
        let _ = signal_hook::flag::register(sig, stop.clone());
    }
    let mut last_save = std::time::Instant::now();
    let mut last_sig = app.snapshot_sig();
    let result = (|| -> io::Result<()> {
        while !app.quit && !stop.load(std::sync::atomic::Ordering::Relaxed) {
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
            // Notepad++ style snapshot: every 7 s, but only when something changed (a crash or killed terminal loses at most 7 s)
            if last_save.elapsed() > Duration::from_secs(7) {
                last_save = std::time::Instant::now();
                let sig = app.snapshot_sig();
                if sig != last_sig { app.save_session(false); last_sig = sig; }
            }
        }
        Ok(())
    })();

    app.save_session(true);
    disable_raw_mode()?;
    execute!(term.backend_mut(), DisableBracketedPaste, DisableMouseCapture, LeaveAlternateScreen)?;
    term.show_cursor()?;
    result
}
