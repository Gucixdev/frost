//! Configuration is a Lua file, ~/.config/frost/init.lua (XDG_CONFIG_HOME, %APPDATA%\frost, ~/Library/Application Support/frost),
//! plus keys.lua next to it, which the key panel writes.
//! It runs once at start with a global table `frost`; nothing else is exposed.
use ratatui::style::Color;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub struct Theme { pub terminal: bool, pub bg: Color, pub bar: Color, pub dim: Color, pub fg: Color, pub accent: Color, pub sel: Color, pub cursor2: Color }

/// Automatic brackets and quotes: `pairs` maps a lower-case language name ("*" = every other) to the opening characters that
/// get a partner, `strict` to "never leave this language with unbalanced brackets".
#[derive(Clone, Default)]
pub struct Pairing { pub off: bool, pub pairs: HashMap<String, String>, pub strict: HashMap<String, bool> }

pub struct Config {
    pub pairing: Pairing,
    pub theme: Theme,
    pub line_numbers: bool,
    pub keys: HashMap<String, String>,
    pub dictionary: Option<PathBuf>,
    pub error: Option<String>,
}

static THEME: OnceLock<Theme> = OnceLock::new();
static PAIRING: OnceLock<Pairing> = OnceLock::new();
static TAB: AtomicUsize = AtomicUsize::new(4);

pub fn theme() -> &'static Theme { THEME.get_or_init(|| preset("terminal")) }
pub fn pairing() -> &'static Pairing { PAIRING.get_or_init(Pairing::default) }
pub fn tab_width() -> usize { TAB.load(Ordering::Relaxed) }

const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }

pub fn preset(name: &str) -> Theme {
    let mut t = preset_rgb(name);
    t.terminal = name != "light" && name != "gruvbox" && name != "mono" && name != "dark";
    t
}

/// The default is the terminal's own palette: default fg/bg and ANSI colours, so frost looks like the rest of the terminal.
fn preset_rgb(name: &str) -> Theme {
    match name {
        "dark" => Theme { terminal: false, bg: rgb(30, 31, 36), bar: rgb(44, 46, 54), dim: rgb(120, 124, 138), fg: rgb(205, 209, 220), accent: rgb(97, 175, 239), sel: rgb(62, 78, 110), cursor2: rgb(229, 192, 123) },
        "light" => Theme { terminal: false, bg: rgb(250, 250, 248), bar: rgb(232, 232, 228), dim: rgb(130, 130, 130), fg: rgb(40, 40, 44), accent: rgb(0, 102, 204), sel: rgb(190, 215, 245), cursor2: rgb(200, 120, 0) },
        "gruvbox" => Theme { terminal: false, bg: rgb(40, 40, 40), bar: rgb(60, 56, 54), dim: rgb(146, 131, 116), fg: rgb(235, 219, 178), accent: rgb(250, 189, 47), sel: rgb(80, 73, 69), cursor2: rgb(251, 73, 52) },
        "mono" => Theme { terminal: false, bg: rgb(0, 0, 0), bar: rgb(30, 30, 30), dim: rgb(120, 120, 120), fg: rgb(220, 220, 220), accent: rgb(255, 255, 255), sel: rgb(70, 70, 70), cursor2: rgb(180, 180, 180) },
        _ => Theme { terminal: true, bg: Color::Reset, bar: Color::DarkGray, dim: Color::DarkGray, fg: Color::Reset, accent: Color::Cyan, sel: Color::Blue, cursor2: Color::Yellow },
    }
}

fn hex(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 { return None; }
    Some(Color::Rgb(u8::from_str_radix(&s[0..2], 16).ok()?, u8::from_str_radix(&s[2..4], 16).ok()?, u8::from_str_radix(&s[4..6], 16).ok()?))
}

pub fn config_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) { return PathBuf::from(x).join("frost"); }
    if cfg!(windows) { if let Some(a) = std::env::var_os("APPDATA") { return PathBuf::from(a).join("frost"); } }
    if cfg!(target_os = "macos") { if let Some(h) = std::env::var_os("HOME") { return PathBuf::from(h).join("Library/Application Support/frost"); } }
    match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) { Some(h) => PathBuf::from(h).join(".config/frost"), None => PathBuf::from(".frost-config") }
}

/// keys.lua: what the key panel (F1) saved; loaded after init.lua, so it has the last word
pub fn keys_file() -> PathBuf { config_dir().join("keys.lua") }

/// replace keys.lua (written next to it and renamed, so a crash never leaves half a file)
pub fn write_keys(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
    let tmp = path.with_extension("lua.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

fn fresh_config() -> Config { Config { pairing: Pairing::default(), theme: preset("terminal"), line_numbers: true, keys: HashMap::new(), dictionary: None, error: None } }

#[cfg(test)]
pub fn test_config() -> Config { fresh_config() }

pub fn load() -> Config {
    let mut cfg = fresh_config();
    let (init, keys) = (std::fs::read_to_string(config_dir().join("init.lua")).ok(), std::fs::read_to_string(keys_file()).ok());
    // keys.lua runs in the same Lua state after init.lua; an error in either shows in the status line
    if init.is_some() || keys.is_some() {
        let (i, k) = (init.as_deref().unwrap_or(""), keys.as_deref().unwrap_or(""));
        if let Err(e) = run_chunks(&[i, k], &mut cfg) {
            // one broken file must not take the other with it: use whichever still runs on its own
            let mut other = fresh_config();
            let which = if run_chunks(&[i], &mut other).is_ok() { other } else { let mut o = fresh_config(); let _ = run_chunks(&[k], &mut o); o };
            cfg = which;
            cfg.error = Some(format!("config: {}", e.to_string().lines().next().unwrap_or("")));
        }
    }
    let _ = THEME.set(cfg.theme);
    let _ = PAIRING.set(cfg.pairing.clone());
    cfg
}

#[cfg(test)]
fn run(src: &str, cfg: &mut Config) -> mlua::Result<()> { run_chunks(&[src], cfg) }

/// run the Lua chunks one after the other in one `frost` table (init.lua, then keys.lua), then read the result
pub fn run_chunks(chunks: &[&str], cfg: &mut Config) -> mlua::Result<()> {
    use mlua::{Lua, Table, Value};
    let lua = Lua::new();
    let frost = lua.create_table()?;
    frost.set("line_numbers", true)?;
    frost.set("tab_width", 4)?;
    frost.set("theme", "terminal")?;
    frost.set("colors", lua.create_table()?)?;
    frost.set("keys", lua.create_table()?)?;
    frost.set("auto_pair", true)?;
    frost.set("pairs", lua.create_table()?)?;
    frost.set("strict", lua.create_table()?)?;
    lua.globals().set("frost", frost.clone())?;
    for src in chunks { lua.load(*src).exec()?; }
    cfg.line_numbers = frost.get::<bool>("line_numbers").unwrap_or(true);
    TAB.store(frost.get::<usize>("tab_width").unwrap_or(4).clamp(1, 16), Ordering::Relaxed);
    let mut t = preset(&frost.get::<String>("theme").unwrap_or_default());
    let custom = frost.get::<Table>("colors").map(|c| c.pairs::<String, String>().count() > 0).unwrap_or(false);
    if let Ok(c) = frost.get::<Table>("colors") {
        for (name, slot) in [("bg", &mut t.bg), ("bar", &mut t.bar), ("dim", &mut t.dim), ("fg", &mut t.fg), ("accent", &mut t.accent), ("sel", &mut t.sel), ("cursor2", &mut t.cursor2)] {
            if let Ok(Value::String(s)) = c.get::<Value>(name) { if let Some(col) = s.to_str().ok().and_then(|s| hex(&s)) { *slot = col; } }
        }
    }
    if custom { t.terminal = false; }
    cfg.theme = t;
    if let Ok(k) = frost.get::<Table>("keys") {
        for pair in k.pairs::<String, String>().flatten() { cfg.keys.insert(pair.0.to_lowercase(), pair.1.to_lowercase()); }
    }
    cfg.pairing.off = !frost.get::<bool>("auto_pair").unwrap_or(true);
    if let Ok(t) = frost.get::<Table>("pairs") { for (k, v) in t.pairs::<String, String>().flatten() { cfg.pairing.pairs.insert(k.to_lowercase(), v); } }
    if let Ok(t) = frost.get::<Table>("strict") { for (k, v) in t.pairs::<String, bool>().flatten() { cfg.pairing.strict.insert(k.to_lowercase(), v); } }
    cfg.dictionary = frost.get::<String>("dictionary").ok().map(PathBuf::from);
    Ok(())
}

/// highlighted row / menu title: reverse video in the terminal theme (readable on any palette), accent colours otherwise
pub fn accent_style() -> ratatui::style::Style {
    use ratatui::style::{Modifier, Style};
    let t = theme();
    if t.terminal { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default().bg(t.accent).fg(t.bg) }
}
/// second and further carets
pub fn caret2_style() -> ratatui::style::Style {
    use ratatui::style::{Modifier, Style};
    let t = theme();
    if t.terminal { Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD) } else { Style::default().bg(t.cursor2).fg(t.bg) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fresh() -> Config { fresh_config() }

    #[test]
    fn lua_sets_options_theme_and_keys() {
        let mut c = fresh();
        run("frost.line_numbers = false; frost.theme = 'light'; frost.colors.accent = '#ff0000'; frost.keys['ctrl-j'] = 'Goto'; frost.dictionary = '/x/y'", &mut c).unwrap();
        assert!(!c.line_numbers);
        assert!(matches!(c.theme.accent, Color::Rgb(255, 0, 0)));
        assert!(matches!(c.theme.bg, Color::Rgb(250, 250, 248)));
        assert_eq!(c.keys["ctrl-j"], "goto");
        assert_eq!(c.dictionary, Some(PathBuf::from("/x/y")));
    }

    #[test]
    fn lua_sets_pairing() {
        let mut c = fresh();
        run("frost.auto_pair = false; frost.pairs['Rust'] = '([{'; frost.strict['Emacs Lisp'] = false", &mut c).unwrap();
        assert!(c.pairing.off);
        assert_eq!(c.pairing.pairs["rust"], "([{");
        assert!(!c.pairing.strict["emacs lisp"]);
        let mut d = fresh();
        run("", &mut d).unwrap();
        assert!(!d.pairing.off && d.pairing.pairs.is_empty());
    }

    #[test]
    fn broken_lua_is_an_error_not_a_crash() {
        let mut c = fresh();
        assert!(run("frost.theme = = 1", &mut c).is_err());
    }
}
