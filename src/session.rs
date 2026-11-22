use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct TabState {
    pub path: Option<String>,
    pub unsaved: Option<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll: usize,
    pub syntax: Option<String>,
    /// undo/redo history (kept across restarts so there is always history)
    #[serde(default)]
    pub history: Option<crate::buffer::History>,
    /// id of the tab's history file under <state>/history/
    #[serde(default)]
    pub hid: String,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct Session {
    pub tabs: Vec<TabState>,
    pub active: usize,
    /// recently closed tabs (newest last), kept across restarts so a closed tab can come back like in Notepad++
    #[serde(default)]
    pub closed: Vec<TabState>,
}

pub fn session_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("frost");
    }
    if cfg!(windows) { if let Some(a) = std::env::var_os("LOCALAPPDATA") { return PathBuf::from(a).join("frost"); } }
    match std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        Some(h) if cfg!(target_os = "macos") => PathBuf::from(h).join("Library/Application Support/frost/state"),
        Some(h) => PathBuf::from(h).join(".local/state/frost"),
        None => PathBuf::from(".frost"),
    }
}

pub fn load() -> Session {
    load_from(&session_dir())
}

pub fn save(s: &Session) -> io::Result<()> {
    save_to(&session_dir(), s)
}

/// Read the session. Never fails: a missing file is an empty session, and a file that cannot be read or parsed is moved
/// aside as `session.json.bad-<unix time>` (see `quarantine`) so the editor starts empty and the next save does not clobber it.
pub fn load_from(dir: &Path) -> Session {
    let path = dir.join("session.json");
    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(session) => session,
            Err(_) => { quarantine(dir); Session::default() }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Session::default(),
        Err(_) => { quarantine(dir); Session::default() }
    }
}

/// Move `session.json` out of the way as `session.json.bad-<unix time>` (a number is added when that name is taken).
/// Returns the new path, or None when there was nothing to move or it could not be moved.
pub fn quarantine(dir: &Path) -> Option<PathBuf> {
    let path = dir.join("session.json");
    fs::symlink_metadata(&path).ok()?;
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    for n in 0..100 {
        let name = if n == 0 { format!("session.json.bad-{ts}") } else { format!("session.json.bad-{ts}-{n}") };
        let to = dir.join(name);
        if fs::symlink_metadata(&to).is_ok() { continue; }
        return fs::rename(&path, &to).ok().map(|_| to);
    }
    None
}

/// a new or emptied file only the user can read: sessions and histories hold unsaved text
pub fn create_private(path: &Path) -> io::Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let f = o.open(path)?;
    #[cfg(unix)]
    f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;   // a file from an older version keeps its place, not its mode
    Ok(f)
}

pub fn save_to(dir: &Path, s: &Session) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp_path = dir.join("session.json.tmp");
    let mut tmp_file = create_private(&tmp_path)?;
    tmp_file.write_all(serde_json::to_string(s)?.as_bytes())?;
    tmp_file.flush()?;
    fs::rename(tmp_path, dir.join("session.json"))?;
    Ok(())
}

pub fn list_named(dir: &Path) -> Vec<String> {
    let sessions_dir = dir.join("sessions");
    if let Ok(entries) = fs::read_dir(sessions_dir) {
        entries
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    e.path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                })
            })
            .collect()
    } else {
        Vec::new()
    }
}

pub fn save_named(dir: &Path, name: &str, s: &Session) -> io::Result<()> {
    if name.contains('/') || name.contains("..") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Name contains invalid characters",
        ));
    }

    let sessions_dir = dir.join("sessions");
    fs::create_dir_all(&sessions_dir)?;
    let path = sessions_dir.join(format!("{}.json", name));
    let mut file = create_private(&path)?;
    file.write_all(serde_json::to_string(s)?.as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn load_named(dir: &Path, name: &str) -> Option<Session> {
    let sessions_dir = dir.join("sessions");
    let path = sessions_dir.join(format!("{}.json", name));
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TmpDir(PathBuf);
    impl TmpDir { fn path(&self) -> &Path { &self.0 } }
    impl Drop for TmpDir { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    fn tempdir() -> io::Result<TmpDir> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!("frost-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&p)?;
        Ok(TmpDir(p))
    }

    #[test]
    fn test_round_trip() {
        let dir = tempdir().unwrap();
        let session = Session {
            closed: vec![],
            tabs: vec![TabState {
                path: Some("test.txt".to_string()),
                unsaved: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll: 0,
                syntax: None,
                history: None,
                hid: String::new(),
            }],
            active: 0,
        };

        save_to(dir.path(), &session).unwrap();
        let loaded = load_from(dir.path());
        assert_eq!(session, loaded);
    }

    #[test]
    fn test_corrupt_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        fs::write(&path, "corrupt data").unwrap();

        let loaded = load_from(dir.path());
        assert_eq!(loaded, Session::default());
        assert!(!path.exists(), "the bad file is moved away");
        let bad: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("session.json.bad-")).collect();
        assert_eq!(bad.len(), 1);
        assert_eq!(fs::read_to_string(bad[0].path()).unwrap(), "corrupt data");
    }

    #[test]
    fn an_unreadable_session_is_moved_aside_too() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("session.json")).unwrap();   // reading a directory fails
        assert_eq!(load_from(dir.path()), Session::default());
        assert!(!dir.path().join("session.json").exists());
        assert!(fs::read_dir(dir.path()).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("session.json.bad-")));
        // nothing to move: no panic, no file made up
        assert_eq!(quarantine(dir.path()), None);
    }

    #[test]
    fn quarantine_never_overwrites_an_earlier_bad_file() {
        let dir = tempdir().unwrap();
        let mut seen = std::collections::HashSet::new();
        for i in 0..3 {
            fs::write(dir.path().join("session.json"), format!("bad {i}")).unwrap();
            assert!(seen.insert(quarantine(dir.path()).unwrap()));
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn test_named_session_round_trip() {
        let dir = tempdir().unwrap();
        let session = Session {
            closed: vec![],
            tabs: vec![TabState {
                path: Some("test.txt".to_string()),
                unsaved: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll: 0,
                syntax: None,
                history: None,
                hid: String::new(),
            }],
            active: 0,
        };

        save_named(dir.path(), "test", &session).unwrap();
        let loaded = load_named(dir.path(), "test").unwrap();
        assert_eq!(session, loaded);
    }

    #[test]
    fn test_bad_name_rejected() {
        let dir = tempdir().unwrap();
        let session = Session::default();

        assert!(save_named(dir.path(), "bad/name", &session).is_err());
        assert!(save_named(dir.path(), "../name", &session).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn session_and_history_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let s = Session { tabs: vec![TabState { unsaved: Some("a password I typed".into()), ..Default::default() }], active: 0, closed: vec![] };
        save_to(&dir.0, &s).unwrap();
        save_named(&dir.0, "n", &s).unwrap();
        let h = crate::buffer::History { base: "secret".into(), steps: vec![], current: 0 };
        crate::history_store::save(&dir.0, "abc123", &h).unwrap();
        for f in ["session.json", "sessions/n.json", "history/abc123.z"] {
            let mode = fs::metadata(dir.0.join(f)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode & 0o077, 0, "{f} is readable by others: {mode:o}");
        }
    }
}
