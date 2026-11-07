//! Syntax highlighting: syntect with the two-face syntax set (Sublime syntaxes as in bat, ~200 languages - wider than Notepad++).
//! A parser checkpoint every CHECK lines, so drawing a window deep in a big file does not reparse from line 0 every frame.
use ratatui::style::{Color, Modifier, Style};
use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter as Hi, Theme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};
use two_face::theme::EmbeddedThemeName;

const CHECK: usize = 128;

pub struct Highlighter {
    ss: SyntaxSet,
    theme: Theme,
    names: Vec<String>,
    cache_syntax: usize,
    /// state at the START of line k*CHECK
    checks: Vec<(ParseState, HighlightState)>,
}

impl Highlighter {
    pub fn new() -> Self {
        let ss = two_face::syntax::extra_newlines();
        let theme = two_face::theme::extra().get(EmbeddedThemeName::Base16OceanDark).clone();
        let names = ss.syntaxes().iter().map(|s| s.name.clone()).collect();
        Highlighter { ss, theme, names, cache_syntax: usize::MAX, checks: vec![] }
    }

    /// all syntax names, sorted, for a menu
    pub fn language_names(&self) -> Vec<String> {
        let mut v = self.names.clone();
        v.sort_by_key(|n| n.to_lowercase());
        v.dedup();
        v
    }

    pub fn name_of(&self, syntax: usize) -> String { self.names.get(syntax).cloned().unwrap_or_else(|| "Plain Text".into()) }
    pub fn index_of(&self, name: &str) -> Option<usize> { self.names.iter().position(|n| n == name) }

    fn plain(&self) -> usize { self.index_of("Plain Text").unwrap_or(0) }

    /// by file name / extension, else by first line (shebang, <?xml ...), else plain text
    pub fn detect(&self, path: Option<&str>, first_line: &str) -> usize {
        let find = |s: &syntect::parsing::SyntaxReference| self.names.iter().position(|n| *n == s.name);
        if let Some(p) = path {
            let p = std::path::Path::new(p);
            let file = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if let Some(s) = self.ss.find_syntax_by_extension(file) { if let Some(i) = find(s) { return i; } }
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if let Some(s) = self.ss.find_syntax_by_extension(ext) { if let Some(i) = find(s) { return i; } }
            }
        }
        if let Some(s) = self.ss.find_syntax_by_first_line(first_line) { if let Some(i) = find(s) { return i; } }
        self.plain()
    }

    /// text changed at `line`: checkpoints that start after it are stale
    pub fn invalidate_from(&mut self, line: usize) {
        let keep = line / CHECK + 1; // the checkpoint at k*CHECK <= line still starts before the change
        let keep = if line % CHECK == 0 && line > 0 { line / CHECK } else { keep };
        self.checks.truncate(keep.max(0));
        if line == 0 { self.checks.clear(); }
    }

    fn fresh(&self, syntax: usize) -> (ParseState, HighlightState) {
        let sref = self.ss.syntaxes().get(syntax).unwrap_or_else(|| self.ss.find_syntax_plain_text());
        let hi = Hi::new(&self.theme);
        (ParseState::new(sref), HighlightState::new(&hi, ScopeStack::new()))
    }

    fn style(s: syntect::highlighting::Style) -> Style {
        let mut st = Style::default().fg(Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b));
        if s.font_style.contains(syntect::highlighting::FontStyle::BOLD) { st = st.add_modifier(Modifier::BOLD); }
        if s.font_style.contains(syntect::highlighting::FontStyle::ITALIC) { st = st.add_modifier(Modifier::ITALIC); }
        if s.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) { st = st.add_modifier(Modifier::UNDERLINED); }
        st
    }

    /// spans of lines first..first+count; `get_line(i)` has no trailing newline
    pub fn styled(&mut self, syntax: usize, line_count: usize, get_line: &dyn Fn(usize) -> String, first: usize, count: usize) -> Vec<Vec<(Style, String)>> {
        let end = (first + count).min(line_count);
        if first >= end { return vec![]; }
        if self.cache_syntax != syntax { self.checks.clear(); self.cache_syntax = syntax; }
        if self.checks.is_empty() { let f = self.fresh(syntax); self.checks.push(f); }
        let hi = Hi::new(&self.theme);
        // walk from the nearest stored checkpoint to the window, storing new checkpoints on the way
        let k = (first / CHECK).min(self.checks.len() - 1);
        let (mut ps, mut hs) = self.checks[k].clone();
        let mut out = vec![];
        for ln in k * CHECK..end {
            if ln % CHECK == 0 && ln / CHECK == self.checks.len() { self.checks.push((ps.clone(), hs.clone())); }
            let mut line = get_line(ln);
            line.push('\n');
            let ops = match ps.parse_line(&line, &self.ss) { Ok(o) => o, Err(_) => vec![] };
            if ln >= first {
                let mut spans: Vec<(Style, String)> = HighlightIterator::new(&mut hs, &ops, &line, &hi)
                    .map(|(s, t)| (Self::style(s), t.to_string())).collect();
                if let Some(last) = spans.last_mut() { last.1 = last.1.trim_end_matches('\n').to_string(); }
                spans.retain(|s| !s.1.is_empty());
                out.push(spans);
            } else {
                // not shown: only advance the highlight state
                for _ in HighlightIterator::new(&mut hs, &ops, &line, &hi) {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_by_extension_and_shebang() {
        let h = Highlighter::new();
        assert_eq!(h.name_of(h.detect(Some("main.rs"), "")), "Rust");
        assert_eq!(h.name_of(h.detect(Some("x/y/tool.py"), "")), "Python");
        assert_eq!(h.name_of(h.detect(None, "#!/bin/bash")), "Bourne Again Shell (bash)");
        assert_eq!(h.name_of(h.detect(Some("notes.unknownext"), "hello")), "Plain Text");
        assert_eq!(h.name_of(h.detect(Some("Makefile"), "")), "Makefile");
    }

    #[test]
    fn many_languages() {
        let h = Highlighter::new();
        let n = h.language_names().len();
        assert!(n >= 100, "only {n} languages");
        for want in ["C", "C++", "Go", "Rust", "Lua", "TOML", "Nim", "Zig", "Lisp", "Haskell"] {
            assert!(h.index_of(want).is_some() || h.language_names().iter().any(|x| x.starts_with(want)), "missing {want}");
        }
    }

    #[test]
    fn spans_and_checkpoints() {
        let mut h = Highlighter::new();
        let rs = h.detect(Some("a.rs"), "");
        let lines: Vec<String> = (0..1000).map(|i| format!("fn f{i}() {{ let x = \"s\"; }} // c")).collect();
        let get = |i: usize| lines[i].clone();
        let top = h.styled(rs, lines.len(), &get, 0, 3);
        assert_eq!(top.len(), 3);
        assert!(top[0].len() > 3, "a Rust line is split into several coloured spans");
        assert_eq!(top[0].iter().map(|s| s.1.as_str()).collect::<String>(), lines[0]);
        let deep = h.styled(rs, lines.len(), &get, 700, 2);
        assert_eq!(deep[1].iter().map(|s| s.1.as_str()).collect::<String>(), lines[701]);
        h.invalidate_from(300);
        let again = h.styled(rs, lines.len(), &get, 700, 2);
        assert_eq!(again, deep);
    }
}
