//! Undo histories live in their own files, `<state>/history/<id>.z` (deflate-compressed JSON), one per tab,
//! so the session file stays small and a history is rewritten only when it changed.
use crate::buffer::History;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// ids are made by the editor; anything else (a hand-edited session file) is refused so it cannot name another path
pub fn valid_id(id: &str) -> bool { !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit()) }

fn file(dir: &Path, id: &str) -> PathBuf { dir.join("history").join(format!("{id}.z")) }

pub fn save(dir: &Path, id: &str, h: &History) -> std::io::Result<()> {
    if !valid_id(id) { return Err(std::io::ErrorKind::InvalidInput.into()); }
    let p = file(dir, id);
    std::fs::create_dir_all(p.parent().unwrap())?;
    let json = serde_json::to_vec(h)?;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(3));
    enc.write_all(&json)?;
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, enc.finish()?)?;
    std::fs::rename(&tmp, &p)
}

pub fn load(dir: &Path, id: &str) -> Option<History> {
    if !valid_id(id) { return None; }
    let bytes = std::fs::read(file(dir, id)).ok()?;
    let mut json = vec![];
    flate2::read::ZlibDecoder::new(&bytes[..]).read_to_end(&mut json).ok()?;
    serde_json::from_slice(&json).ok()
}

/// delete history files nobody refers to any more (closed long ago, or from tabs that no longer exist)
pub fn prune(dir: &Path, keep: &HashSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir.join("history")) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let id = name.strip_suffix(".z").unwrap_or(&name);
        if !keep.contains(id) { let _ = std::fs::remove_file(e.path()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Delta;
    fn tmp() -> PathBuf { let d = std::env::temp_dir().join(format!("frost-hist-{}-{:?}", std::process::id(), std::thread::current().id())); let _ = std::fs::remove_dir_all(&d); d }

    #[test]
    fn roundtrip_is_compressed_and_prune_removes_orphans() {
        let d = tmp();
        let h = History { base: "x".repeat(10_000), steps: vec![Delta { start: 0, del: 0, ins: "abc".into() }], current: 1 };
        save(&d, "abc123", &h).unwrap();
        save(&d, "def456", &h).unwrap();
        assert!(std::fs::metadata(file(&d, "abc123")).unwrap().len() < 1000, "10 KB of x must compress");
        assert_eq!(load(&d, "abc123"), Some(h));
        prune(&d, &["abc123".to_string()].into_iter().collect());
        assert!(load(&d, "abc123").is_some() && load(&d, "def456").is_none());
        assert!(save(&d, "../evil", &History { base: String::new(), steps: vec![], current: 0 }).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
