//! Benchmarks, run by `./bench` (release build, `--ignored`): they print numbers and only fail on a gross regression.
use crate::buffer::Buffer;
use std::time::{Duration, Instant};

fn report(what: &str, t: Duration) { println!("bench {what:<48} {:>9.1} ms", t.as_secs_f64() * 1e3); }

fn big_text(mb: usize) -> String {
    let line = "fn main() { let value = compute(42, \"text\"); println!(\"{value}\"); } // line of source text\n";
    line.repeat(mb * 1024 * 1024 / line.len())
}

#[test]
#[ignore]
fn open_100mb_file() {
    let dir = std::env::temp_dir().join(format!("frost-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("big.rs");
    std::fs::write(&p, big_text(100)).unwrap();
    let t = Instant::now();
    let mut b = Buffer::open(&p).unwrap();
    let open = t.elapsed();
    report("open 100 MB file (first screen ready)", open);
    let t = Instant::now();
    b.ensure_loaded();
    report("load the rest of the 100 MB file", t.elapsed());
    let t = Instant::now();
    let n = b.contents().len();
    report("contents() of 100 MB", t.elapsed());
    assert!(n > 90 << 20);
    assert!(open < Duration::from_secs(10), "{open:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore]
fn insert_at_1000_cursors() {
    let text = "word ".repeat(200_000);
    let t = Instant::now();
    let mut n = 0;
    // carets spread over the file: one "Z" per 200 words, then select every occurrence of it
    let mut spread = String::new();
    for (i, w) in text.split(' ').enumerate() { if i > 0 && i % 200 == 0 { spread.push_str("Z "); } else { spread.push_str(w); spread.push(' '); } }
    let mut z = Buffer::from_text(&spread, None);
    z.cursors = vec![crate::buffer::Cursor { head: spread.find('Z').unwrap() + 1, anchor: spread.find('Z').unwrap(), want: None }];
    n += z.select_all_occurrences();
    report(&format!("select {n} occurrences in {} KB", spread.len() / 1024), t.elapsed());
    let t = Instant::now();
    z.insert("ZZ");
    let ins = t.elapsed();
    report(&format!("insert at {n} cursors"), ins);
    let t = Instant::now();
    assert!(z.undo());
    report("undo that insert", t.elapsed());
    assert!(n >= 1000);
    assert!(ins < Duration::from_secs(2), "{ins:?}");
}

#[test]
#[ignore]
fn merge3_on_a_big_file() {
    let base = big_text(20);
    let lines: Vec<&str> = base.split_inclusive('\n').collect();
    let (mut mine, mut theirs) = (String::new(), String::new());
    for (i, l) in lines.iter().enumerate() {
        mine.push_str(if i % 1000 == 0 { "// mine\n" } else { l });
        theirs.push_str(if i % 1000 == 500 { "// theirs\n" } else { l });
    }
    let t = Instant::now();
    let (m, c) = crate::merge::merge3(&base, &mine, &theirs);
    let el = t.elapsed();
    report(&format!("merge3 20 MB, {} lines, {} edits per side", lines.len(), lines.len() / 1000), el);
    assert_eq!(c, 0);
    assert!(m.contains("// mine") && m.contains("// theirs"));
    assert!(el < Duration::from_secs(30), "{el:?}");
}
