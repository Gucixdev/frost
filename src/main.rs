//! frost - a small terminal editor. Fresh-like layout, micro-like keys, CUA + mouse + multi-cursor, Notepad++-style sessions.
mod app;
mod buffer;
mod config;
mod dict;
mod diff;
mod highlight;
mod history_store;
mod keys;
mod lang;
#[cfg(test)]
mod bench;
mod session;
mod ui;

use crossterm::event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

enum Wait { Ready, Timeout, Hup }
/// wait up to `ms` for input on stdin; reports a hung-up terminal instead of letting crossterm spin on it
#[cfg(unix)]
fn stdin_is_tty() -> bool { unsafe { libc::isatty(0) == 1 } }
#[cfg(unix)]
fn wait_tty(ms: u64) -> Wait {
    // stdin is a pipe or /dev/null (a launcher, `cmd | frost`): crossterm reads /dev/tty then, and a closed pipe on fd 0 says nothing about the terminal
    if !stdin_is_tty() { return if event::poll(Duration::from_millis(ms)).unwrap_or(false) { Wait::Ready } else { Wait::Timeout }; }
    let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    let r = unsafe { libc::poll(&mut p, 1, ms as i32) };
    if r < 0 { return Wait::Timeout; }   // EINTR (a signal): the loop re-checks the stop flag
    if p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 { Wait::Hup }
    else if p.revents & libc::POLLIN != 0 { Wait::Ready } else { Wait::Timeout }
}
#[cfg(not(unix))]
fn wait_tty(ms: u64) -> Wait { if event::poll(Duration::from_millis(ms)).unwrap_or(false) { Wait::Ready } else { Wait::Timeout } }

/// true when the terminal on stdin has hung up (crossterm would spin on read()==0 forever, leaving an orphan process)
#[cfg(unix)]
fn tty_hung_up() -> bool {
    if !stdin_is_tty() { return false; }
    let mut p = libc::pollfd { fd: 0, events: 0, revents: 0 };
    unsafe { libc::poll(&mut p, 1, 0) >= 0 && p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 }
}
#[cfg(not(unix))]
fn tty_hung_up() -> bool { false }

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("frost [FILE...]\n  no files: reopen the last session (open tabs, unsaved text, cursors)\n  F1 keys, F10 menu, Ctrl-P command palette, Ctrl-Q quit (the session is saved on quit)");
        return Ok(());
    }
    // restore the terminal even when something panics (installed first: App::new reads the saved session, and a panic that
    // is caught on purpose, see app::guarded, stays silent)
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        if app::quiet_panics() { return; }
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture, LeaveAlternateScreen);
        hook(p);
    }));
    // a panic while building the app (a file argument, config) must not be the end either: one retry without the saved session
    let mut app = match app::guarded(|| app::App::new(&args)) {
        Ok(a) => a,
        Err(why) => {
            let kept = session::quarantine(&session::session_dir());
            let mut a = app::guarded(|| app::App::new(&args)).unwrap_or_else(|_| app::App::blank());
            a.msg = format!("start failed ({why}), saved session {}", kept.and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned())).unwrap_or_else(|| "ignored".into()));
            a
        }
    };

    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    // a shutdown or a closed terminal (SIGTERM/SIGHUP) saves the session and the undo history like a normal quit
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP, signal_hook::consts::SIGINT] {
        let _ = signal_hook::flag::register(sig, stop.clone());
    }
    let mut last_save = std::time::Instant::now();
    let mut last_sig = app.snapshot_sig();
    let (mut frames, mut size_errors) = (0u32, 0u32);
    let result = (|| -> io::Result<()> {
        while !app.quit && !stop.load(std::sync::atomic::Ordering::Relaxed) {
            // the terminal is gone (closed window, dead ssh, killed multiplexer): quit instead of spinning as an orphan
            if tty_hung_up() { break; }
            // no usable size (0 rows or columns before the window is configured, tput missing): wait, do not draw and do not quit
            let usable = match crossterm::terminal::size() { Ok((w, h)) => { size_errors = 0; w > 0 && h > 0 } Err(_) => { size_errors += 1; false } };
            if size_errors > 40 { break; }
            if usable {
                frames += 1;
                if frames <= 5 {
                    // the first frames draw whatever the saved session restored: a panic there moves the session aside and starts empty
                    let drawn = app::guarded(|| term.draw(|f| ui::draw(f, &mut app)).map(|_| ()));
                    if let Ok(Err(e)) = &drawn { return Err(io::Error::new(e.kind(), e.to_string())); }
                    if let Err(why) = drawn {
                        let kept = session::quarantine(&session::session_dir());
                        app = app::guarded(|| app::App::new(&args)).unwrap_or_else(|_| app::App::blank());
                        app.msg = format!("drawing the saved session failed ({why}), kept as {}", kept.and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned())).unwrap_or_else(|| "(not kept)".into()));
                        last_sig = app.snapshot_sig();
                    }
                } else { term.draw(|f| ui::draw(f, &mut app))?; }
            }
            let busy = app.tick();
            let ms = if busy { 40 } else { 500 };
            // wait on the fd ourselves first: crossterm's poll/read never return once the terminal has hung up
            let ready = match wait_tty(ms) { Wait::Hup => break, Wait::Timeout => false, Wait::Ready => true };
            if ready && event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
                    Event::Paste(s) => app.on_paste(&s),
                    _ => {}
                }
                // drain what is already waiting (fast typing, pastes) before drawing again
                                while !tty_hung_up() && !stop.load(std::sync::atomic::Ordering::Relaxed) && event::poll(Duration::from_millis(0))? && !tty_hung_up() {
                    match event::read()? {
                        Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                        Event::Mouse(m) => app.on_mouse(m),
                        Event::Paste(s) => app.on_paste(&s),
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
