//! Word dictionary in the spirit of TempleOS: while you type, words that start the same way are offered
//! with their meaning; F2 explains the word under the cursor.
//! Sources, merged: the built-in list, `dictionary` from init.lua or ~/.config/frost/dictionary.txt
//! (one entry per line: `word<TAB>meaning`, or just `word`), and /usr/share/dict/words when it exists.
use std::path::PathBuf;

pub struct Dict {
    /// (lowercase key, word, meaning) sorted by key
    entries: Vec<(String, String, String)>,
}

impl Dict {
    pub fn load(extra: Option<PathBuf>) -> Dict {
        let mut d = Dict { entries: vec![] };
        d.add_text(include_str!("../data/dictionary.tsv"));
        let mut files = vec![];
        if let Some(p) = extra { files.push(p); }
        files.push(crate::config::config_dir().join("dictionary.txt"));
        files.push(PathBuf::from("/usr/share/dict/words"));
        for f in files { if let Ok(t) = std::fs::read_to_string(f) { d.add_text(&t); } }
        d.entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.2.len().cmp(&a.2.len())));   // same word: the one with a meaning first
        d.entries.dedup_by(|b, a| a.0 == b.0);
        d
    }

    pub fn add_text(&mut self, t: &str) {
        for line in t.lines() {
            let (w, m) = line.split_once('\t').unwrap_or((line, ""));
            let w = w.trim();
            if w.len() >= 2 && !w.contains(' ') { self.entries.push((w.to_lowercase(), w.to_string(), m.trim().to_string())); }
        }
    }

    /// up to `max` words starting with `prefix` (case-insensitive), `local` words (from the text being edited) first
    pub fn complete(&self, prefix: &str, local: &[String], max: usize) -> Vec<(String, String)> {
        let p = prefix.to_lowercase();
        let mut out: Vec<(String, String)> = vec![];
        for w in local {
            if w.len() > p.len() && w.to_lowercase().starts_with(&p) && !out.iter().any(|o| &o.0 == w) {
                out.push((w.clone(), self.define(w).unwrap_or_default()));
            }
            if out.len() >= max { return out; }
        }
        let start = self.entries.partition_point(|e| e.0.as_str() < p.as_str());
        for e in &self.entries[start..] {
            if !e.0.starts_with(&p) { break; }
            if e.0 != p && !out.iter().any(|o| o.0.to_lowercase() == e.0) { out.push((e.1.clone(), e.2.clone())); }
            if out.len() >= max { break; }
        }
        out
    }

    pub fn define(&self, word: &str) -> Option<String> {
        let k = word.to_lowercase();
        let i = self.entries.binary_search_by(|e| e.0.as_str().cmp(&k)).ok()?;
        Some(self.entries[i].2.clone()).filter(|m| !m.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d() -> Dict { let mut d = Dict { entries: vec![] }; d.add_text("buffer\ta block of memory\nbuffalo\nbug\ta mistake\nzebra"); d.entries.sort_by(|a, b| a.0.cmp(&b.0)); d }

    #[test]
    fn prefix_lookup_is_sorted_and_case_insensitive() {
        let r = d().complete("BUF", &[], 10);
        assert_eq!(r.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["buffalo", "buffer"]);
    }
    #[test]
    fn local_words_come_first_and_have_meanings() {
        let r = d().complete("bu", &["bugfix".to_string()], 3);
        assert_eq!(r[0].0, "bugfix");
        assert_eq!(d().define("bug").as_deref(), Some("a mistake"));
        assert_eq!(d().define("zebra"), None);
    }
    #[test]
    fn exact_word_is_not_offered_again() { assert!(d().complete("bug", &[], 5).is_empty()); }
}
