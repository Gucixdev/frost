//! What frost knows about a language while you type: which brackets pair up, whether unbalanced brackets are forbidden, where
//! a bracket is real (not inside a string or a comment), what one level of indentation is, and how Lisp lines up.
use ropey::Rope;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Lisp, CStyle, Hash, Prose, Other }

#[derive(Clone, PartialEq, Debug)]
pub struct Lang {
    pub name: String,
    pub kind: Kind,
    /// opening characters that get a partner when typed: any of ( [ { " '
    pub pairs: String,
    /// never leave the text with unbalanced brackets
    pub strict: bool,
    /// a line ending in ':' opens a block (Python, YAML)
    pub colon: bool,
    /// ' starts a string, not a character literal (JavaScript, PHP, CSS...)
    pub sq_string: bool,
    /// the language insists on tab characters (Go, Makefile)
    pub tabs: bool,
}

impl Default for Lang {
    fn default() -> Self { Lang { name: String::new(), kind: Kind::Other, pairs: "([{\"'".into(), strict: false, colon: false, sq_string: false, tabs: false } }
}

fn kind_of(n: &str) -> Kind {
    let is = |l: &[&str]| l.contains(&n);
    if n.contains("lisp") || n.contains("scheme") || n.contains("clojure") || n.contains("racket") { Kind::Lisp }
    else if is(&["plain text", "text", "markdown", "restructuredtext", "latex", "tex", "asciidoc", "org", "git commit", "log"]) || n.starts_with("markdown") { Kind::Prose }
    else if is(&["c", "c++", "c#", "rust", "java", "go", "swift", "kotlin", "scala", "php", "dart", "zig", "objective-c", "objective-c++", "glsl", "hlsl", "groovy", "solidity", "jsx", "tsx", "d", "v"])
        || n.starts_with("javascript") || n.starts_with("typescript") { Kind::CStyle }
    else if is(&["python", "ruby", "perl", "yaml", "toml", "makefile", "r", "julia", "nim", "elixir", "dockerfile", "cmake", "gdscript", "crystal"]) { Kind::Hash }
    else { Kind::Other }
}

impl Lang {
    pub fn of(name: &str) -> Lang {
        let n = name.to_lowercase();
        let kind = kind_of(&n);
        let cfg = crate::config::pairing();
        let default = match kind { Kind::Lisp | Kind::Prose => "([{\"", _ if n == "rust" => "([{\"", _ => "([{\"'" };
        let pairs = if cfg.off { String::new() } else { cfg.pairs.get(&n).or_else(|| cfg.pairs.get("*")).cloned().unwrap_or_else(|| default.into()) };
        let strict = cfg.strict.get(&n).copied().unwrap_or(kind == Kind::Lisp && !cfg.off);
        Lang {
            kind, pairs, strict, name: name.to_string(),
            colon: matches!(n.as_str(), "python" | "gdscript" | "yaml" | "nim"),
            sq_string: n.starts_with("javascript") || n.starts_with("typescript") || matches!(n.as_str(), "php" | "dart" | "jsx" | "tsx" | "groovy"),
            tabs: matches!(n.as_str(), "go" | "makefile"),
        }
    }
    /// brackets are matched and highlighted
    pub fn matches_brackets(&self) -> bool { self.kind != Kind::Prose }
    /// unmatched brackets are reported (only where strings and comments are understood well enough to be sure)
    pub fn reports_unbalanced(&self) -> bool { matches!(self.kind, Kind::Lisp | Kind::CStyle | Kind::Hash) }
}

pub fn closer_of(c: char) -> Option<char> { match c { '(' => Some(')'), '[' => Some(']'), '{' => Some('}'), _ => None } }
pub fn opener_of(c: char) -> Option<char> { match c { ')' => Some('('), ']' => Some('['), '}' => Some('{'), _ => None } }
pub fn is_bracket(c: char) -> bool { closer_of(c).is_some() || opener_of(c).is_some() }
pub fn is_quote(c: char) -> bool { c == '"' || c == '\'' }

// ------------------------------------------------------------------ finding brackets
/// bigger texts are not scanned (a keystroke must stay cheap)
const LIMIT: usize = 512 << 10;

#[derive(Default)]
pub struct Scan {
    /// open <-> close position of every matched bracket, both directions
    pub partner: HashMap<usize, usize>,
    /// unclosed openers and closers without an opener, in text order
    pub bad: Vec<(usize, char)>,
    /// false: this language or size is not checked, `bad` is empty without meaning "balanced"
    pub checked: bool,
}

fn eol(cs: &[char], mut i: usize) -> usize { while i < cs.len() && cs[i] != '\n' { i += 1; } i }

/// index after the closing `q` of a string that starts at `i`; a one-line string ends at the line end, as an unterminated one
fn string_end(cs: &[char], i: usize, q: char, multiline: bool) -> usize {
    let mut j = i + 1;
    while j < cs.len() {
        match cs[j] {
            '\\' => j += 2,
            c if c == q => return j + 1,
            '\n' if !multiline => return j,
            _ => j += 1,
        }
    }
    cs.len()
}

fn find_seq(cs: &[char], mut i: usize, seq: &[char]) -> usize {
    while i + seq.len() <= cs.len() { if cs[i..i + seq.len()] == *seq { return i + seq.len(); } i += 1; }
    cs.len()
}

/// every bracket outside strings and comments: (position, char)
fn brackets(cs: &[char], lang: &Lang) -> Vec<(usize, char)> {
    let at = |i: usize| cs.get(i).copied().unwrap_or('\0');
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if lang.kind != Kind::Other {
            match (lang.kind, c) {
                (Kind::Lisp, ';') | (Kind::Hash, '#') => { i = eol(cs, i); continue; }
                (Kind::Lisp, '#') if at(i + 1) == '\\' => { i += 3; continue; }
                (Kind::Lisp, '#') if at(i + 1) == '|' => { i = find_seq(cs, i + 2, &['|', '#']); continue; }
                (Kind::CStyle, '/') if at(i + 1) == '/' => { i = eol(cs, i); continue; }
                (Kind::CStyle, '/') if at(i + 1) == '*' => { i = find_seq(cs, i + 2, &['*', '/']); continue; }
                (Kind::CStyle, '`') => { i = string_end(cs, i, '`', true); continue; }
                (Kind::CStyle, '\'') if !lang.sq_string => {
                    // 'x' and '\n' are characters, 'a without a closing quote is a lifetime or an apostrophe
                    if at(i + 1) == '\\' { let mut j = i + 3; while j < cs.len() && cs[j] != '\'' && j < i + 12 { j += 1; } i = j + 1; }
                    else if at(i + 2) == '\'' { i += 3; } else { i += 1; }
                    continue;
                }
                (_, '"') | (_, '\'') if !(lang.kind == Kind::Lisp && c == '\'') => {
                    if lang.kind == Kind::Hash && at(i + 1) == c && at(i + 2) == c { i = find_seq(cs, i + 3, &[c, c, c]); }
                    else { i = string_end(cs, i, c, lang.kind == Kind::Lisp); }
                    continue;
                }
                _ => {}
            }
        }
        if is_bracket(c) { out.push((i, c)); }
        i += 1;
    }
    out
}

/// match brackets with a stack; returns the pairs and what is left over
fn pair_up(bs: &[(usize, char)]) -> (HashMap<usize, usize>, Vec<(usize, char)>, Vec<(usize, char)>) {
    let (mut partner, mut stack, mut stray) = (HashMap::new(), Vec::<(usize, char)>::new(), vec![]);
    for &(p, c) in bs {
        match opener_of(c) {
            None => stack.push((p, c)),
            // a closer takes the nearest open bracket of its kind; openers left above it are unclosed
            Some(o) => match stack.iter().rposition(|&(_, oc)| oc == o) {
                Some(k) => {
                    let q = stack[k].0;
                    stray.extend(stack.drain(k + 1..));
                    stack.pop();
                    partner.insert(p, q);
                    partner.insert(q, p);
                }
                None => stray.push((p, c)),
            },
        }
    }
    (partner, stack, stray)
}

pub fn scan(text: &Rope, lang: &Lang) -> Scan {
    if !lang.matches_brackets() || text.len_chars() > LIMIT { return Scan::default(); }
    let cs: Vec<char> = text.chars().collect();
    let (partner, open, stray) = pair_up(&brackets(&cs, lang));
    let mut bad: Vec<(usize, char)> = open.into_iter().chain(stray).collect();
    bad.sort();
    Scan { partner, bad, checked: lang.reports_unbalanced() }
}

/// the brackets still open just before `pos`, innermost last
pub fn open_at(text: &Rope, lang: &Lang, pos: usize) -> Vec<(usize, char)> {
    if !lang.matches_brackets() || text.len_chars() > LIMIT { return vec![]; }
    let cs: Vec<char> = text.slice(..pos.min(text.len_chars())).chars().collect();
    pair_up(&brackets(&cs, lang)).1
}

// ------------------------------------------------------------------ indentation
/// one level of indentation as the file already does it (tabs or N spaces); a file without indentation gets the language's
/// habit (tabs for Go, Makefile) or the configured tab_width
pub fn indent_unit(text: &Rope, lang: &Lang) -> String {
    let (mut tabs, mut spaces, mut prev) = (0usize, 0usize, 0usize);
    let mut hist = [0usize; 9];
    for line in text.lines().take(500) {
        let lead: Vec<char> = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        if lead.len() == line.len_chars() || line.len_chars() == lead.len() + 1 && line.char(lead.len()) == '\n' { continue; }
        if lead.first() == Some(&'\t') { tabs += 1; prev = 0; continue; }
        if !lead.is_empty() { spaces += 1; }
        let d = lead.len().abs_diff(prev);
        if (1..=8).contains(&d) { hist[d] += 1; }
        prev = lead.len();
    }
    if tabs > spaces { return "\t".into(); }
    let best = (1..=8).filter(|&d| hist[d] > 0).fold(None, |b: Option<usize>, d| match b { Some(x) if hist[x] >= hist[d] => Some(x), _ => Some(d) });
    match best {
        Some(w) if spaces > 0 => " ".repeat(w),
        _ if lang.tabs => "\t".into(),
        _ => " ".repeat(crate::config::tab_width()),
    }
}

/// leading blanks of line `l`
pub fn line_indent(text: &Rope, l: usize) -> String { text.line(l).chars().take_while(|c| *c == ' ' || *c == '\t').collect() }

/// the column a new line at `pos` gets in Lisp: under the first argument of the innermost open form, two columns in for
/// forms that take a body (def..., let, lambda, when...), one in for lists and data
pub fn lisp_indent(text: &Rope, lang: &Lang, pos: usize) -> usize {
    let Some(&(op, ch)) = open_at(text, lang, pos).last() else { return 0 };
    let l = text.char_to_line(op);
    let ls = text.line_to_char(l);
    let col = op - ls;
    if ch != '(' { return col + 1; }
    let end = (text.line_to_char(l) + text.line(l).len_chars()).min(pos).max(op + 1);
    let line: Vec<char> = text.slice(op + 1..end).chars().filter(|c| *c != '\n').collect();
    let word_end = line.iter().position(|c| c.is_whitespace() || is_bracket(*c) || *c == ';').unwrap_or(line.len());
    if word_end == 0 { return col + 1; }
    let head: String = line[..word_end].iter().collect();
    let body = head.starts_with("def") || head.starts_with("with-") || head.starts_with("do") || head.starts_with("let")
        || matches!(head.as_str(), "lambda" | "when" | "unless" | "progn" | "prog1" | "cond" | "case" | "while" | "loop" | "begin" | "fn" | "if-let" | "when-let" | "ns" | "try" | "for" | "catch" | "unwind-protect" | "save-excursion" | "condition-case" | "match");
    if body { return col + 2; }
    // the second thing on the opening line, if there is one
    match line[word_end..].iter().position(|c| !c.is_whitespace()) {
        Some(k) if line[word_end + k] != ';' => col + 1 + word_end + k,
        _ => col + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang(n: &str) -> Lang { Lang::of(n) }
    fn bad(src: &str, l: &str) -> Vec<(usize, char)> { scan(&Rope::from_str(src), &lang(l)).bad }

    #[test]
    fn languages_get_sane_defaults() {
        assert_eq!(lang("Rust").pairs, "([{\"");
        assert_eq!(lang("C").pairs, "([{\"'");
        assert_eq!(lang("Plain Text").pairs, "([{\"");
        assert_eq!(lang("Markdown").kind, Kind::Prose);
        assert_eq!(lang("Emacs Lisp").kind, Kind::Lisp);
        assert!(lang("Clojure").strict && lang("Lisp").strict);
        assert!(!lang("Rust").strict && !lang("Python").strict);
        assert!(lang("Python").colon && lang("Go").tabs);
        assert_eq!(lang("JavaScript (Babel)").kind, Kind::CStyle);
    }

    #[test]
    fn strings_comments_and_literals_do_not_count() {
        assert!(bad("fn f() { let s = \"(\"; } // )\n/* ( */ let c = '(';", "Rust").is_empty());
        assert!(bad("fn f<'a>(x: &'a str) -> &'a str { x }", "Rust").is_empty(), "lifetimes are not quotes");
        assert!(bad("let q = '\\'' + '}';", "Rust").is_empty());
        assert!(bad("x = [1, 2] # )\ns = \"(\" + '['\n\"\"\"\n(\n\"\"\"", "Python").is_empty());
        assert!(bad("(define (f x) ; (\n  \"a ) string\" #\\( #\\))", "Lisp").is_empty());
        assert!(bad("(a #| ( |# b)", "Lisp").is_empty());
        assert!(bad("var s = 'it(' + `a\n(`; f()", "JavaScript").is_empty());
    }

    #[test]
    fn unbalanced_brackets_are_found() {
        assert_eq!(bad("fn main() {\n  f(1;\n", "Rust"), vec![(10, '{'), (15, '(')]);
        assert_eq!(bad("a) (b", "Rust"), vec![(1, ')'), (3, '(')]);
        assert_eq!(bad("(a [b)]", "Lisp"), vec![(3, '['), (6, ']')]);
        let s = scan(&Rope::from_str("f(x)[1]"), &lang("C"));
        assert!(s.checked && s.bad.is_empty());
        assert_eq!((s.partner[&1], s.partner[&3], s.partner[&4], s.partner[&6]), (3, 1, 6, 4));
    }

    #[test]
    fn prose_other_and_huge_texts_are_not_judged() {
        assert!(!scan(&Rope::from_str("1) one (two"), &lang("Plain Text")).checked);
        let s = scan(&Rope::from_str("case x in a) ;; (b"), &lang("Bash"));
        assert!(!s.checked && s.bad.len() == 2, "matching only, no verdict");
        assert!(!scan(&Rope::from_str(&"(".repeat(LIMIT + 1)), &lang("Lisp")).checked);
    }

    #[test]
    fn open_brackets_at_a_position() {
        let t = Rope::from_str("fn f() { g(1, [2, \"(\"");
        let o = open_at(&t, &lang("Rust"), t.len_chars());
        assert_eq!(o.iter().map(|x| x.1).collect::<String>(), "{([");
    }

    #[test]
    fn indent_unit_follows_the_file() {
        let u = |s: &str, l: &str| indent_unit(&Rope::from_str(s), &lang(l));
        assert_eq!(u("fn f() {\n\tx;\n\t{\n\t\ty;\n\t}\n}\n", "Rust"), "\t");
        assert_eq!(u("a:\n  b:\n    c: 1\n  d: 2\n", "YAML"), "  ");
        assert_eq!(u("def f():\n    if x:\n        y\n    z\n", "Python"), "    ");
        assert_eq!(u("one\ntwo\n", "Go"), "\t");
        assert_eq!(u("one\ntwo\n", "Rust"), " ".repeat(crate::config::tab_width()));
        assert_eq!(u("x\n   y\n   z\n", "Rust"), "   ");
    }

    #[test]
    fn lisp_lines_up_like_emacs() {
        let at = |s: &str| { let t = Rope::from_str(s); lisp_indent(&t, &lang("Lisp"), t.len_chars()) };
        assert_eq!(at("(foo bar\n"), 5, "under the first argument");
        assert_eq!(at("(foo\n"), 1);
        assert_eq!(at("(defun f (x)\n"), 2, "body forms: two in");
        assert_eq!(at("(let ((a 1)\n      (b 2))\n"), 2);
        assert_eq!(at("(let ((a 1)\n"), 6, "inside the binding list: under the first binding");
        assert_eq!(at("(a (b c)\n"), 3);
        assert_eq!(at("  [1 2\n"), 3);
        assert_eq!(at("(foo \"a)\"\n"), 5);
        assert_eq!(at("(a b)\n"), 0);
    }
}
