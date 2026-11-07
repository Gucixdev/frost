//! Differences between two texts, shown as an ordinary unified diff in a tab (so it can be searched and scrolled like any text).
use similar::TextDiff;

/// unified diff of `a` -> `b`, empty string when they are equal
pub fn unified(a_name: &str, a: &str, b_name: &str, b: &str) -> String {
    if a == b { return String::new(); }
    TextDiff::from_lines(a, b).unified_diff().context_radius(3).header(a_name, b_name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_texts_give_nothing() { assert_eq!(unified("a", "x\n", "b", "x\n"), ""); }
    #[test]
    fn changed_line_shows_minus_and_plus() {
        let d = unified("old", "one\ntwo\nthree\n", "new", "one\n2\nthree\n");
        assert!(d.contains("--- old") && d.contains("+++ new"));
        assert!(d.contains("-two\n") && d.contains("+2\n"));
        assert!(d.contains(" one\n"));
    }
}
