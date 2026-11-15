//! Three-way merge of text by lines: `base` is the text both sides started from, `mine` the editor's text,
//! `theirs` the new text on disk. Edits on different lines merge cleanly; edits that touch the same lines
//! of `base` differently become a conflict with markers.
use similar::{DiffTag, TextDiff};

/// one side's change: replace base lines [s, e) by `new`
struct Change<'a> { s: usize, e: usize, new: Vec<&'a str> }

fn lines(t: &str) -> Vec<&str> { t.split_inclusive('\n').collect() }

fn changes<'a>(base: &str, other: &'a str, ol: &[&'a str]) -> Vec<Change<'a>> {
    let mut v = Vec::new();
    for op in TextDiff::from_lines(base, other).ops() {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal { continue; }
        v.push(Change { s: o.start, e: o.end, new: ol[n.start.min(ol.len())..n.end.min(ol.len())].to_vec() });
    }
    v
}

/// base lines [rs, re) with this side's changes applied
fn side(bl: &[&str], rs: usize, re: usize, cs: &[Change]) -> String {
    let mut out = String::new();
    let mut at = rs;
    for c in cs {
        for l in &bl[at..c.s] { out.push_str(l); }
        for l in &c.new { out.push_str(l); }
        at = c.e;
    }
    for l in &bl[at..re] { out.push_str(l); }
    out
}

fn with_newline(mut s: String) -> String {
    if !s.is_empty() && !s.ends_with('\n') { s.push('\n'); }
    s
}

/// Merge `mine` and `theirs`, both derived from `base`. Returns the merged text and the number of conflicts;
/// a conflict is written as `<<<<<<< yours` / `=======` / `>>>>>>> disk`.
pub fn merge3(base: &str, mine: &str, theirs: &str) -> (String, usize) {
    if mine == theirs { return (mine.to_string(), 0); }
    if base == mine { return (theirs.to_string(), 0); }
    if base == theirs { return (mine.to_string(), 0); }
    let (bl, ml, tl) = (lines(base), lines(mine), lines(theirs));
    let (a, b) = (changes(base, mine, &ml), changes(base, theirs, &tl));
    let (mut ia, mut ib) = (0, 0);
    let (mut out, mut conflicts, mut pos) = (String::new(), 0, 0);
    while ia < a.len() || ib < b.len() {
        let take_a = ib >= b.len() || (ia < a.len() && a[ia].s <= b[ib].s);
        let first = if take_a { &a[ia] } else { &b[ib] };
        let (rs, mut re) = (first.s, first.e);
        let (sa, sb) = (ia, ib);
        loop {
            let mut grew = false;
            if ia < a.len() && (a[ia].s < re || a[ia].s == rs) { re = re.max(a[ia].e); ia += 1; grew = true; }
            if ib < b.len() && (b[ib].s < re || b[ib].s == rs) { re = re.max(b[ib].e); ib += 1; grew = true; }
            if !grew { break; }
        }
        for l in &bl[pos.min(rs)..rs] { out.push_str(l); }
        pos = re;
        let (ca, cb) = (&a[sa..ia], &b[sb..ib]);
        if cb.is_empty() { out.push_str(&side(&bl, rs, re, ca)); }
        else if ca.is_empty() { out.push_str(&side(&bl, rs, re, cb)); }
        else {
            let (x, y) = (side(&bl, rs, re, ca), side(&bl, rs, re, cb));
            if x == y { out.push_str(&x); } else {
                conflicts += 1;
                out.push_str("<<<<<<< yours\n");
                out.push_str(&with_newline(x));
                out.push_str("=======\n");
                out.push_str(&with_newline(y));
                out.push_str(">>>>>>> disk\n");
            }
        }
    }
    for l in &bl[pos.min(bl.len())..] { out.push_str(l); }
    (out, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_edits_merge() {
        let base = "a\nb\nc\nd\ne\n";
        let (m, c) = merge3(base, "A\nb\nc\nd\ne\n", "a\nb\nc\nd\nE\n");
        assert_eq!((m.as_str(), c), ("A\nb\nc\nd\nE\n", 0));
    }

    #[test]
    fn adjacent_lines_merge() {
        let (m, c) = merge3("a\nb\nc\n", "A\nb\nc\n", "a\nB\nc\n");
        assert_eq!((m.as_str(), c), ("A\nB\nc\n", 0));
    }

    #[test]
    fn identical_edit_on_both_sides() {
        let (m, c) = merge3("a\nb\nc\n", "a\nX\nc\n", "a\nX\nc\n");
        assert_eq!((m.as_str(), c), ("a\nX\nc\n", 0));
        let (m, c) = merge3("a\nb\nc\nd\n", "a\nX\nc\nM\n", "a\nX\nc\nd\n");
        assert_eq!((m.as_str(), c), ("a\nX\nc\nM\n", 0));
    }

    #[test]
    fn real_conflict() {
        let (m, c) = merge3("a\nb\nc\n", "a\nmine\nc\n", "a\ntheirs\nc\n");
        assert_eq!(c, 1);
        assert_eq!(m, "a\n<<<<<<< yours\nmine\n=======\ntheirs\n>>>>>>> disk\nc\n");
    }

    #[test]
    fn insert_at_same_point_with_different_text() {
        let (m, c) = merge3("a\nc\n", "a\nmine\nc\n", "a\ntheirs\nc\n");
        assert_eq!(c, 1);
        assert_eq!(m, "a\n<<<<<<< yours\nmine\n=======\ntheirs\n>>>>>>> disk\nc\n");
    }

    #[test]
    fn insert_at_different_points_merges() {
        let (m, c) = merge3("a\nb\nc\n", "x\na\nb\nc\n", "a\nb\nc\ny\n");
        assert_eq!((m.as_str(), c), ("x\na\nb\nc\ny\n", 0));
    }

    #[test]
    fn delete_versus_edit_conflicts() {
        let (m, c) = merge3("a\nb\nc\n", "a\nc\n", "a\nB\nc\n");
        assert_eq!(c, 1);
        assert_eq!(m, "a\n<<<<<<< yours\n=======\nB\n>>>>>>> disk\nc\n");
    }

    #[test]
    fn only_theirs_or_only_mine() {
        assert_eq!(merge3("a\nb\n", "a\nb\n", "a\nB\n"), ("a\nB\n".to_string(), 0));
        assert_eq!(merge3("a\nb\n", "a\nM\n", "a\nb\n"), ("a\nM\n".to_string(), 0));
    }

    #[test]
    fn no_newline_at_end_of_file() {
        let (m, c) = merge3("a\nb\nc", "A\nb\nc", "a\nb\nC");
        assert_eq!((m.as_str(), c), ("A\nb\nC", 0));
        let (m, c) = merge3("a\nb", "a\nmine", "a\ntheirs");
        assert_eq!(c, 1);
        assert_eq!(m, "a\n<<<<<<< yours\nmine\n=======\ntheirs\n>>>>>>> disk\n");
    }

    #[test]
    fn empty_base() {
        assert_eq!(merge3("", "m\n", "m\n"), ("m\n".to_string(), 0));
        assert_eq!(merge3("", "", "t\n"), ("t\n".to_string(), 0));
        assert_eq!(merge3("", "m\n", ""), ("m\n".to_string(), 0));
        let (m, c) = merge3("", "m\n", "t\n");
        assert_eq!(c, 1);
        assert_eq!(m, "<<<<<<< yours\nm\n=======\nt\n>>>>>>> disk\n");
    }

    #[test]
    fn several_regions_in_one_merge() {
        let base = "1\n2\n3\n4\n5\n6\n7\n";
        let (m, c) = merge3(base, "1\nM\n3\n4\n5\n6\n7\n", "1\n2\n3\n4\nT\n6\nT7\n");
        assert_eq!((m.as_str(), c), ("1\nM\n3\n4\nT\n6\nT7\n", 0));
    }
}
