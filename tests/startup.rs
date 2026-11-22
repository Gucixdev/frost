//! The real binary in a pty: it must start (and stay up until Ctrl-Q) however its terminal or its saved session looks.
//! Own XDG dirs under the system temp dir, so the user's real session is never touched. Unix only.
#![cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
enum Stdin { Tty, ClosedPipe }

struct Run { alive: bool, quit_ok: bool, out: String }

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("frost-startup-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("state/frost")).unwrap();
    d
}

fn run(home: &Path, rows: u16, cols: u16, stdin: Stdin) -> Run {
    let (mut m, mut s) = (0, 0);
    let ws = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
    assert_eq!(unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null(), &ws) }, 0);
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_frost"));
    cmd.env_clear().env("TERM", "xterm-256color").env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home.join("home")).env("XDG_STATE_HOME", home.join("state")).env("XDG_DATA_HOME", home.join("data")).env("XDG_CONFIG_HOME", home.join("config"));
    let mut closed_pipe = None;
    cmd.stdin(match stdin {
        Stdin::Tty => Stdio::from(slave.try_clone().unwrap()),
        Stdin::ClosedPipe => { let (r, w) = std::io::pipe().unwrap(); drop(w); closed_pipe = Some(()); Stdio::from(r) }
    });
    let _ = closed_pipe;
    cmd.stdout(Stdio::from(slave.try_clone().unwrap())).stderr(Stdio::from(slave.try_clone().unwrap()));
    let sfd = slave.as_raw_fd();
    unsafe { cmd.pre_exec(move || { libc::setsid(); libc::ioctl(sfd, libc::TIOCSCTTY as _, 0); Ok(()) }); }
    let mut child = cmd.spawn().unwrap();
    drop(slave);
    let mfd = master.as_raw_fd();
    let mut out = Vec::new();
    let drain = |out: &mut Vec<u8>, ms: u64| {
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(ms) {
            let mut p = libc::pollfd { fd: mfd, events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut p, 1, 50) } > 0 && p.revents & libc::POLLIN != 0 {
                let mut b = [0u8; 8192];
                let n = unsafe { libc::read(mfd, b.as_mut_ptr().cast(), b.len()) };
                if n > 0 { out.extend_from_slice(&b[..n as usize]); }
            }
        }
    };
    drain(&mut out, 1200);
    let alive = child.try_wait().unwrap().is_none();
    let mut quit_ok = false;
    if alive {
        unsafe { libc::write(mfd, b"\x11".as_ptr().cast(), 1); }
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) {
            drain(&mut out, 100);
            if let Some(st) = child.try_wait().unwrap() { quit_ok = st.success(); break; }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Run { alive, quit_ok, out: String::from_utf8_lossy(&out).into_owned() }
}

fn assert_starts(r: &Run) {
    assert!(r.alive, "frost exited at once: {:?}", r.out.chars().rev().take(300).collect::<String>().chars().rev().collect::<String>());
    assert!(r.quit_ok, "frost did not quit cleanly on Ctrl-Q");
}

fn bad_dirs(d: &Path) -> Vec<String> {
    std::fs::read_dir(d.join("state/frost")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("session.json.bad-")).collect()
}

#[test]
fn starts_with_stdin_a_closed_pipe() {
    let d = tmp("pipe");
    assert_starts(&run(&d, 40, 120, Stdin::ClosedPipe));
}

#[test]
fn starts_in_a_window_with_zero_rows() {
    let d = tmp("zerorows");
    assert_starts(&run(&d, 0, 120, Stdin::Tty));
}

#[test]
fn starts_with_a_session_full_of_bad_tabs() {
    let d = tmp("badtabs");
    let f = d.join("ok.txt");
    std::fs::write(&f, "hello\n").unwrap();
    let huge = u64::MAX;
    let hist = format!(r#"{{"base":"hello\n","steps":[{{"start":{huge},"del":{huge},"ins":"x"}}],"current":1}}"#);
    let tabs = format!(
        r#"{{"tabs":[
          {{"path":"{p}/missing.txt","unsaved":null,"cursor_line":0,"cursor_col":0,"scroll":0}},
          {{"path":"{p}","unsaved":null,"cursor_line":0,"cursor_col":0,"scroll":0}},
          {{"path":"{f}","unsaved":null,"cursor_line":{huge},"cursor_col":{huge},"scroll":{huge},"history":{hist},"hid":"abc"}},
          {{"path":"{f}","unsaved":"x\ny","cursor_line":{huge},"cursor_col":0,"scroll":{huge},"hid":"abc"}}],
          "active":{huge},"closed":[]}}"#,
        p = d.display(), f = f.display());
    std::fs::write(d.join("state/frost/session.json"), tabs).unwrap();
    assert_starts(&run(&d, 40, 120, Stdin::Tty));
    assert!(bad_dirs(&d).is_empty(), "a session that merely has bad tabs is kept, not set aside");
    // the quit saved a session again
    assert!(std::fs::read_to_string(d.join("state/frost/session.json")).unwrap().contains("ok.txt"));
}

#[test]
fn a_corrupt_session_file_is_moved_aside_and_frost_starts_empty() {
    let d = tmp("corrupt");
    std::fs::write(d.join("state/frost/session.json"), "{ this is not json").unwrap();
    assert_starts(&run(&d, 40, 120, Stdin::Tty));
    let bad = bad_dirs(&d);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(std::fs::read_to_string(d.join("state/frost").join(&bad[0])).unwrap(), "{ this is not json");
}

#[test]
fn a_session_json_that_is_a_directory_does_not_stop_the_start() {
    let d = tmp("dir");
    std::fs::create_dir(d.join("state/frost/session.json")).unwrap();
    assert_starts(&run(&d, 40, 120, Stdin::Tty));
}
