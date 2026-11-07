//! Configuration is a Lua file, ~/.config/frost/init.lua (XDG_CONFIG_HOME, %APPDATA%\frost, ~/Library/Application Support/frost).
//! It runs once at start with a global table `frost`; nothing else is exposed.
use ratatui::style::Color;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub struct Theme { pub bg: Color, pub bar: Color, pub dim: Color, pub fg: Color, pub accent: Color, pub sel: Color, pub cursor2: Color }

pub struct Config {
    pub theme: Theme,
    pub line_numbers: bool,
    pub keys: HashMap<String, String>,
    pub dictionary: Option<PathBuf>,
    pub error: Option<String>,
}

static THEME: OnceLock<Theme> = OnceLock::new();
static TAB: AtomicUsize = AtomicUsize::new(4);

pub fn theme() -> &'static Theme { THEME.get_or_init(|| preset("dark")) }
pub fn tab_width() -> usize { TAB.load(Ordering::Relaxed) }

const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }

pub fn preset(name: &str) -> Theme {
    match name {
        "light" => Theme { bg: rgb(250, 250, 248), bar: rgb(232, 232, 228), dim: rgb(130, 130, 130), fg: rgb(40, 40, 44), accent: rgb(0, 102, 204), sel: rgb(190, 215, 245), cursor2: rgb(200, 120, 0) },
        "gruvbox" => Theme { bg: rgb(40, 40, 40), bar: rgb(60, 56, 54), dim: rgb(146, 131, 116), fg: rgb(235, 219, 178), accent: rgb(250, 189, 47), sel: rgb(80, 73, 69), cursor2: rgb(251, 73, 52) },
        "mono" => Theme { bg: rgb(0, 0, 0), bar: rgb(30, 30, 30), dim: rgb(120, 120, 120), fg: rgb(220, 220, 220), accent: rgb(255, 255, 255), sel: rgb(70, 70, 70), cursor2: rgb(180, 180, 180) },
        _ => Theme { bg: rgb(30, 31, 36), bar: rgb(44, 46, 54), dim: rgb(120, 124, 138), fg: rgb(205, 209, 220), accent: rgb(97, 175, 239), sel: rgb(62, 78, 110), cursor2: rgb(229, 192, 123) },
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

pub fn load() -> Config {
    let mut cfg = Config { theme: preset("dark"), line_numbers: true, keys: HashMap::new(), dictionary: None, error: None };
    let path = config_dir().join("init.lua");
    if let Ok(src) = std::fs::read_to_string(&path) {
        if let Err(e) = run(&src, &mut cfg) { cfg.error = Some(format!("init.lua: {}", e.to_string().lines().next().unwrap_or(""))); }
    }
    let _ = THEME.set(cfg.theme);
    cfg
}

fn run(src: &str, cfg: &mut Config) -> mlua::Result<()> {
    use mlua::{Lua, Table, Value};
    let lua = Lua::new();
    let frost = lua.create_table()?;
    frost.set("line_numbers", true)?;
    frost.set("tab_width", 4)?;
    frost.set("theme", "dark")?;
    frost.set("colors", lua.create_table()?)?;
    frost.set("keys", lua.create_table()?)?;
    lua.globals().set("frost", frost.clone())?;
    lua.load(src).exec()?;
    cfg.line_numbers = frost.get::<bool>("line_numbers").unwrap_or(true);
    TAB.store(frost.get::<usize>("tab_width").unwrap_or(4).clamp(1, 16), Ordering::Relaxed);
    let mut t = preset(&frost.get::<String>("theme").unwrap_or_default());
    if let Ok(c) = frost.get::<Table>("colors") {
        for (name, slot) in [("bg", &mut t.bg), ("bar", &mut t.bar), ("dim", &mut t.dim), ("fg", &mut t.fg), ("accent", &mut t.accent), ("sel", &mut t.sel), ("cursor2", &mut t.cursor2)] {
            if let Ok(Value::String(s)) = c.get::<Value>(name) { if let Some(col) = s.to_str().ok().and_then(|s| hex(&s)) { *slot = col; } }
        }
    }
    cfg.theme = t;
    if let Ok(k) = frost.get::<Table>("keys") {
        for pair in k.pairs::<String, String>().flatten() { cfg.keys.insert(pair.0.to_lowercase(), pair.1.to_lowercase()); }
    }
    cfg.dictionary = frost.get::<String>("dictionary").ok().map(PathBuf::from);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fresh() -> Config { Config { theme: preset("dark"), line_numbers: true, keys: HashMap::new(), dictionary: None, error: None } }

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
    fn broken_lua_is_an_error_not_a_crash() {
        let mut c = fresh();
        assert!(run("frost.theme = = 1", &mut c).is_err());
    }
}
