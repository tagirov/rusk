//! User configuration: `~/.config/rusk/cfg` (see CONFIG.md).
//!
//! A deliberately tiny hand-rolled `key = value` format instead of TOML:
//! the target syntax uses bare unquoted values (`list_header = blue`,
//! `priority_marker = accent`) which are invalid TOML, and supports
//! user-defined variables resolved top-to-bottom — with zero new
//! dependencies and line-numbered warnings.
//!
//! Parse problems are never fatal: every issue becomes a warning and the
//! built-in default is kept, so a broken theme can not lock the user out
//! of their tasks. Environment variables always win over config values.

use colored::{Color, ColoredString, Colorize};
use std::path::PathBuf;
use std::sync::OnceLock;

/// The 16 ANSI colors: config name, terminal color, canonical CSS hex
/// (xterm defaults) used by the web frontend.
const PALETTE: [(&str, Color, &str); 16] = [
    ("black", Color::Black, "#000000"),
    ("red", Color::Red, "#cd0000"),
    ("green", Color::Green, "#00cd00"),
    ("yellow", Color::Yellow, "#cdcd00"),
    ("blue", Color::Blue, "#0000ee"),
    ("magenta", Color::Magenta, "#cd00cd"),
    ("cyan", Color::Cyan, "#00cdcd"),
    ("white", Color::White, "#e5e5e5"),
    ("bright black", Color::BrightBlack, "#7f7f7f"),
    ("bright red", Color::BrightRed, "#ff0000"),
    ("bright green", Color::BrightGreen, "#00ff00"),
    ("bright yellow", Color::BrightYellow, "#ffff00"),
    ("bright blue", Color::BrightBlue, "#5c5cff"),
    ("bright magenta", Color::BrightMagenta, "#ff00ff"),
    ("bright cyan", Color::BrightCyan, "#00ffff"),
    ("bright white", Color::BrightWhite, "#ffffff"),
];

/// A theme color: a named ANSI-16 color (index into [`PALETTE`]), a truecolor
/// RGB value from `#rrggbb`, or the terminal's default foreground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorValue {
    Named(u8),
    Rgb(u8, u8, u8),
    /// No coloring: the terminal's default foreground (e.g. `task_id`).
    Default,
}

impl ColorValue {
    /// Parses an ANSI-16 color name (`red`, `bright_black`, `purple` alias)
    /// or a `#rrggbb` hex value. Never falls back silently: unknown names
    /// are an error (unlike `colored`'s `From<&str>`, which yields White).
    pub fn parse(s: &str) -> Result<Self, String> {
        let norm = s.trim().to_ascii_lowercase();
        if let Some(hex) = norm.strip_prefix('#') {
            if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
                return Ok(ColorValue::Rgb(byte(0), byte(2), byte(4)));
            }
            return Err(format!("invalid hex color '{s}' (expected #rrggbb)"));
        }
        let name = norm.replace('_', " ");
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        let name = if name == "purple" {
            "magenta".to_string()
        } else {
            name
        };
        if let Some(idx) = PALETTE.iter().position(|(n, _, _)| *n == name) {
            return Ok(ColorValue::Named(idx as u8));
        }
        Err(format!(
            "unknown color '{s}' (expected one of the 16 ANSI names or #rrggbb)"
        ))
    }

    /// The `colored` color to render with; `None` for the terminal default.
    pub fn to_colored(self) -> Option<Color> {
        match self {
            ColorValue::Named(i) => Some(PALETTE[i as usize].1),
            ColorValue::Rgb(r, g, b) => Some(Color::TrueColor { r, g, b }),
            ColorValue::Default => None,
        }
    }

    /// CSS hex for the web frontend; named colors map through the canonical
    /// xterm palette. `None` for the terminal default (template fallback wins).
    pub fn to_css_hex(self) -> Option<String> {
        match self {
            ColorValue::Named(i) => Some(PALETTE[i as usize].2.to_string()),
            ColorValue::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
            ColorValue::Default => None,
        }
    }

    /// Applies this color to `s` (no-op coloring for [`ColorValue::Default`]).
    /// Styles chain as usual: `theme().task_id.paint("1").bold()`.
    pub fn paint(self, s: &str) -> ColoredString {
        match self.to_colored() {
            Some(color) => s.color(color),
            None => s.normal(),
        }
    }
}

/// Defines [`Theme`] with per-key defaults plus key-string lookup used by the
/// parser (`set`) and the web frontend (`entries`).
macro_rules! theme_keys {
    ( $( $field:ident => $default:expr ),+ $(,)? ) => {
        /// Semantic color groups; defaults reproduce the previously
        /// hardcoded colors exactly.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Theme {
            $( pub $field: ColorValue, )+
        }

        impl Default for Theme {
            fn default() -> Self {
                Self { $( $field: $default, )+ }
            }
        }

        impl Theme {
            pub const KEYS: &'static [&'static str] = &[ $( stringify!($field), )+ ];

            fn get(&self, key: &str) -> Option<ColorValue> {
                match key {
                    $( stringify!($field) => Some(self.$field), )+
                    _ => None,
                }
            }

            fn set(&mut self, key: &str, value: ColorValue) {
                match key {
                    $( stringify!($field) => self.$field = value, )+
                    _ => unreachable!("unknown theme key '{key}'"),
                }
            }

            /// `(key, color)` pairs for iteration (web CSS variables).
            pub fn entries(&self) -> Vec<(&'static str, ColorValue)> {
                vec![ $( (stringify!($field), self.$field), )+ ]
            }
        }
    };
}

theme_keys! {
    error => ColorValue::Named(1),            // red
    success => ColorValue::Named(2),          // green
    warning => ColorValue::Named(3),          // yellow
    notice => ColorValue::Named(5),           // magenta
    info => ColorValue::Named(6),             // cyan
    emphasis => ColorValue::Named(7),         // white
    accent => ColorValue::Rgb(255, 165, 0),   // prompts, counters
    list_header => ColorValue::Named(4),      // blue
    done_marker => ColorValue::Named(2),      // green ✔
    priority_marker => ColorValue::Rgb(255, 165, 0),
    task_id => ColorValue::Default,           // bold only
    search_match => ColorValue::Named(3),     // yellow `rusk search` highlight
    keyword => ColorValue::Named(3),          // yellow TEMP / INFO tokens
    date_overdue => ColorValue::Named(1),     // red
    date_upcoming => ColorValue::Named(6),    // cyan
    date_today => ColorValue::Named(6),       // cyan (same as upcoming by default)
    editor_date => ColorValue::Named(2),      // green first-line date
    editor_footer => ColorValue::Rgb(128, 128, 128),
    editor_dirty => ColorValue::Rgb(185, 185, 190),
    editor_clean => ColorValue::Rgb(100, 100, 100),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Database location: a path, `https://host` (backend-http) or
    /// `[user@]host:path` (backend-ssh) — see `crate::location`.
    pub rusk_db: Option<String>,
    pub db_token: Option<String>,
    pub git_backend: bool,
    pub no_color: bool,
    pub compact: bool,
    pub backup: bool,
    pub web_host: String,
    pub web_port: u16,
    pub web_token: Option<String>,
    pub sync_remote: Option<String>,
    pub sync_token: Option<String>,
    /// Task-text keyword tokens highlighted in the list (`keyword` theme color).
    pub keywords: Vec<String>,
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rusk_db: None,
            db_token: None,
            git_backend: false,
            no_color: false,
            compact: false,
            backup: true,
            web_host: "127.0.0.1".to_string(),
            web_port: 7272,
            web_token: None,
            sync_remote: None,
            sync_token: None,
            keywords: ["TEMP", "INFO", "FIXME", "WIP"]
                .map(String::from)
                .to_vec(),
            theme: Theme::default(),
        }
    }
}

/// Non-theme setting keys recognized by the parser.
const SETTINGS: &[&str] = &[
    "rusk_db",
    "db_token",
    "git_backend",
    "no_color",
    "compact",
    "backup",
    "web_host",
    "web_port",
    "web_token",
    "sync_remote",
    "sync_token",
    "keywords",
];

fn is_setting(key: &str) -> bool {
    SETTINGS.contains(&key) || Theme::KEYS.contains(&key)
}

fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Splits a raw (already trimmed) value into its content and whether it was
/// quoted. Quoted values are literal: no comment stripping, no variable
/// resolution. In bare values an inline comment starts at the first `#`
/// preceded by whitespace — a leading `#` (hex colors) is part of the value.
fn parse_value(raw: &str) -> (String, bool) {
    if let Some(rest) = raw.strip_prefix('"') {
        let content = match rest.find('"') {
            Some(end) => &rest[..end],
            None => rest,
        };
        return (content.to_string(), true);
    }
    let mut cut = raw.len();
    let mut prev_is_space = false;
    for (i, c) in raw.char_indices() {
        if c == '#' && prev_is_space {
            cut = i;
            break;
        }
        prev_is_space = c.is_whitespace();
    }
    (raw[..cut].trim_end().to_string(), false)
}

/// A non-empty environment variable wins over the config value. One that
/// is not valid UTF-8 is an error: taking it for unset would quietly use
/// the config value — another server, another token — instead.
#[cfg(feature = "backend-http")]
pub(crate) fn env_or_config(
    env: &str,
    config_value: &Option<String>,
) -> anyhow::Result<Option<String>> {
    match std::env::var_os(env).filter(|v| !v.is_empty()) {
        Some(value) => match value.into_string() {
            Ok(value) => Ok(Some(value)),
            Err(_) => anyhow::bail!("{env} is not valid UTF-8"),
        },
        None => Ok(config_value.clone()),
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true or false, got '{value}'")),
    }
}

#[derive(Default)]
pub struct LoadOutcome {
    pub config: Config,
    pub warnings: Vec<String>,
}

struct Var {
    name: String,
    value: String,
    line: usize,
    referenced: bool,
}

fn apply_setting(
    config: &mut Config,
    key: &str,
    value: &str,
    line: usize,
    warnings: &mut Vec<String>,
) {
    let mut warn = |msg: String| warnings.push(format!("cfg:{line}: {msg}; using default"));
    // Only `keywords` accepts an empty value (`keywords =` clears the list).
    if value.is_empty() && key != "keywords" {
        warn(format!("empty value for '{key}'"));
        return;
    }
    match key {
        "rusk_db" => config.rusk_db = Some(value.to_string()),
        "db_token" => config.db_token = Some(value.to_string()),
        "no_color" | "compact" | "backup" | "git_backend" => match parse_bool(value) {
            Ok(b) => match key {
                "no_color" => config.no_color = b,
                "compact" => config.compact = b,
                "git_backend" => config.git_backend = b,
                _ => config.backup = b,
            },
            Err(e) => warn(format!("invalid value for '{key}': {e}")),
        },
        "web_host" => config.web_host = value.to_string(),
        "web_port" => match value.parse::<u16>() {
            Ok(p) => config.web_port = p,
            Err(_) => warn(format!("invalid port '{value}' for 'web_port'")),
        },
        "web_token" => config.web_token = Some(value.to_string()),
        "sync_remote" => config.sync_remote = Some(value.to_string()),
        "sync_token" => config.sync_token = Some(value.to_string()),
        // Space- or comma-separated tokens; an empty value (`keywords =`)
        // disables highlighting.
        "keywords" => {
            config.keywords = value
                .split([',', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
        }
        theme_key => match ColorValue::parse(value) {
            Ok(color) => config.theme.set(theme_key, color),
            // A color value may also reference another theme key
            // (e.g. `priority_marker = accent`), copying its current value.
            Err(e) => match config.theme.get(value) {
                Some(color) => config.theme.set(theme_key, color),
                None => warn(format!("{e} for '{theme_key}'")),
            },
        },
    }
}

/// Parses config text into a [`Config`]. Pure and never fatal: anything
/// unparseable keeps its default and is reported in `warnings`.
pub fn parse(text: &str) -> LoadOutcome {
    let mut config = Config::default();
    let mut warnings = Vec::new();
    let mut vars: Vec<Var> = Vec::new();

    for (i, raw_line) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw_line.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(eq) = line.find('=') else {
            warnings.push(format!("cfg:{line_no}: expected `key = value`; line skipped"));
            continue;
        };
        let key = line[..eq].trim();
        if !is_valid_key(key) {
            warnings.push(format!("cfg:{line_no}: invalid key '{key}'; line skipped"));
            continue;
        }
        let (value, quoted) = parse_value(line[eq + 1..].trim());
        // Settings decide themselves what an empty value means
        // (`keywords =` clears the list); an empty variable is a mistake.
        if value.is_empty() && !is_setting(key) {
            warnings.push(format!("cfg:{line_no}: empty value for '{key}'; line skipped"));
            continue;
        }

        // Bare values resolve against previously defined variables.
        let resolved = if quoted {
            value
        } else if let Some(var) = vars.iter_mut().find(|v| v.name == value) {
            var.referenced = true;
            var.value.clone()
        } else {
            value
        };

        if is_setting(key) {
            // The literal `default` keeps the built-in value, so the shipped
            // template can document every setting without changing behavior.
            if !quoted && resolved == "default" {
                continue;
            }
            apply_setting(&mut config, key, &resolved, line_no, &mut warnings);
        } else if let Some(var) = vars.iter_mut().find(|v| v.name == key) {
            var.value = resolved;
            var.line = line_no;
        } else {
            vars.push(Var {
                name: key.to_string(),
                value: resolved,
                line: line_no,
                referenced: false,
            });
        }
    }

    for var in &vars {
        if !var.referenced {
            warnings.push(format!(
                "cfg:{}: unused variable '{}' — did you mean a setting?",
                var.line, var.name
            ));
        }
    }

    LoadOutcome { config, warnings }
}

/// Template written on first run. The two active lines reproduce the built-in
/// defaults exactly (and demonstrate referencing another theme key).
const DEFAULT_CONFIG: &str = "\
# rusk configuration file (auto-created; safe to edit or delete).
# Docs: CONFIG.md — https://github.com/tagirov/rusk/blob/main/CONFIG.md
#
# Syntax: `key = value`, one per line; `#` starts a comment.
# Any key that is not a recognized setting defines a variable; a later bare
# value that matches a variable name resolves to it. The literal value
# `default` keeps the built-in default.
# Colors: one of the 16 ANSI names (black, red, green, yellow, blue, magenta,
# cyan, white, bright_black, bright_red, ...) or a hex value like #ffa500.
# Environment variables always override this file:
#   RUSK_CONFIG, RUSK_DB, RUSK_DB_TOKEN, RUSK_NO_COLOR / NO_COLOR,
#   RUSK_SYNC_REMOTE, RUSK_SYNC_TOKEN.

# --- variables (any unrecognized key defines one) ---
# my_accent = #d75f00

# --- behavior ---
# rusk_db = default          # database location. A path: the extension picks the format
#                            # (.json default; .csv, .md, .txt (todo.txt), .ndjson/.jsonl,
#                            # .ics, .db/.sqlite/.sqlite3 — each needs its build feature).
#                            # Remote: https://host (rusk serve API) or user@host:/path (ssh).
# db_token =                 # Bearer token when rusk_db is an http(s) location
# git_backend = false        # commit every save of a local file database to a git repo
#                            # in the database directory (history + undo; needs `git`)
# no_color = false           # disable ANSI colors in terminal output
# compact = false            # compact `rusk list` view by default
# backup = true              # keep a .backup copy next to the database on every save
# keywords = TEMP INFO FIXME WIP
#                            # keywords highlighted when a task text starts with one
#                            # (space- or comma-separated, case-sensitive;
#                            # `keywords =` with no value disables)

# --- web: rusk serve / rusk gen ---
# web_host = 127.0.0.1
# web_port = 7272
# web_token =                # required when web_host is not localhost

# --- sync: rusk sync ---
# sync_remote =              # user@host:/path/tasks.json (ssh) or https://host (rusk serve API)
# sync_token =               # Bearer token for http(s) remotes

# --- theme ---
# A color value is an ANSI name, a hex #rrggbb, a variable, or another theme key.
accent = #ffa500
priority_marker = accent
# error = red
# success = green
# warning = yellow
# notice = magenta
# info = cyan
# emphasis = white
# list_header = blue
# done_marker = green
# task_id = default
# keyword = yellow
# date_overdue = red
# date_upcoming = cyan
# date_today = cyan
# editor_date = green
# editor_footer = #808080
# editor_dirty = #b9b9be
# editor_clean = #646464
";

/// Default config path: `<config_dir>/rusk/cfg`
/// (`~/.config/rusk/cfg` on Linux).
pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rusk").join("cfg"))
}

fn load_from(path: PathBuf) -> LoadOutcome {
    if !path.exists() {
        // Best-effort auto-creation: the commented template doubles as docs.
        let created = path
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&path, DEFAULT_CONFIG).map(Some));
        if let Err(e) = created {
            return LoadOutcome {
                warnings: vec![format!(
                    "could not create config file {}: {e}",
                    path.display()
                )],
                ..LoadOutcome::default()
            };
        }
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text),
        Err(e) => LoadOutcome {
            warnings: vec![format!(
                "could not read config file {}: {e}",
                path.display()
            )],
            ..LoadOutcome::default()
        },
    }
}

/// Resolves the config path and loads it, auto-creating the default file
/// when missing. `RUSK_CONFIG` overrides the path; when set but empty the
/// config system is disabled (defaults only — used by the test harness).
/// Without `RUSK_CONFIG`, test/debug runs never touch the real config.
pub fn load() -> LoadOutcome {
    // `var_os`: a path does not have to be UTF-8, and one that is not must
    // not be taken for "unset" and replaced by the default config.
    match std::env::var_os("RUSK_CONFIG") {
        Some(path) if path.is_empty() => LoadOutcome::default(),
        Some(path) => load_from(PathBuf::from(path)),
        None => {
            if crate::is_test_mode() || cfg!(debug_assertions) {
                return LoadOutcome::default();
            }
            match default_config_path() {
                Some(path) => load_from(path),
                None => LoadOutcome::default(),
            }
        }
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Stores the loaded config for global access. A second call is a no-op.
pub fn init(config: Config) {
    let _ = CONFIG.set(config);
}

/// The process-wide config; defaults when [`init`] was never called
/// (library consumers, unit tests).
pub fn config() -> &'static Config {
    CONFIG.get_or_init(Config::default)
}

pub fn theme() -> &'static Theme {
    &config().theme
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(text: &str) -> Config {
        parse(text).config
    }

    fn warnings(text: &str) -> Vec<String> {
        parse(text).warnings
    }

    #[test]
    fn defaults_pin_previous_hardcoded_colors() {
        let t = Theme::default();
        assert_eq!(t.error, ColorValue::Named(1));
        assert_eq!(t.success, ColorValue::Named(2));
        assert_eq!(t.warning, ColorValue::Named(3));
        assert_eq!(t.notice, ColorValue::Named(5));
        assert_eq!(t.info, ColorValue::Named(6));
        assert_eq!(t.emphasis, ColorValue::Named(7));
        assert_eq!(t.accent, ColorValue::Rgb(255, 165, 0));
        assert_eq!(t.list_header, ColorValue::Named(4));
        assert_eq!(t.done_marker, ColorValue::Named(2));
        assert_eq!(t.priority_marker, ColorValue::Rgb(255, 165, 0));
        assert_eq!(t.task_id, ColorValue::Default);
        assert_eq!(t.date_overdue, ColorValue::Named(1));
        assert_eq!(t.date_upcoming, ColorValue::Named(6));
        assert_eq!(t.date_today, ColorValue::Named(6));
        assert_eq!(t.editor_date, ColorValue::Named(2));
        assert_eq!(t.editor_footer, ColorValue::Rgb(128, 128, 128));
        assert_eq!(t.editor_dirty, ColorValue::Rgb(185, 185, 190));
        assert_eq!(t.editor_clean, ColorValue::Rgb(100, 100, 100));
    }

    #[test]
    fn default_template_parses_cleanly_and_keeps_defaults() {
        let outcome = parse(DEFAULT_CONFIG);
        assert_eq!(outcome.warnings, Vec::<String>::new());
        assert_eq!(outcome.config, Config::default());
    }

    #[test]
    fn color_names_and_hex() {
        assert_eq!(ColorValue::parse("red"), Ok(ColorValue::Named(1)));
        assert_eq!(ColorValue::parse("BRIGHT_BLACK"), Ok(ColorValue::Named(8)));
        assert_eq!(ColorValue::parse("bright black"), Ok(ColorValue::Named(8)));
        assert_eq!(ColorValue::parse("purple"), Ok(ColorValue::Named(5)));
        assert_eq!(
            ColorValue::parse("#ffa500"),
            Ok(ColorValue::Rgb(255, 165, 0))
        );
        assert!(ColorValue::parse("#fff").is_err());
        assert!(ColorValue::parse("#ffa500aa").is_err());
        assert!(ColorValue::parse("blu").is_err());
    }

    #[test]
    fn css_hex_mapping() {
        assert_eq!(ColorValue::Named(1).to_css_hex().unwrap(), "#cd0000");
        assert_eq!(
            ColorValue::Rgb(255, 165, 0).to_css_hex().unwrap(),
            "#ffa500"
        );
        assert_eq!(ColorValue::Default.to_css_hex(), None);
    }

    #[test]
    fn comments_and_blank_lines() {
        let c = cfg("# full comment\n\n  \nerror = green # inline\n");
        assert_eq!(c.theme.error, ColorValue::Named(2));
    }

    #[test]
    fn hex_value_is_not_a_comment() {
        let c = cfg("accent = #cd00cd\n");
        assert_eq!(c.theme.accent, ColorValue::Rgb(0xcd, 0x00, 0xcd));
    }

    #[test]
    fn quoted_values_are_literal() {
        let c = cfg("rusk_db = \"/tmp/my # tasks/db.json\"\n");
        assert_eq!(c.rusk_db.unwrap(), "/tmp/my # tasks/db.json");
    }

    #[test]
    fn variables_resolve_and_chain() {
        let text = "base = cyan\nalias = base\nerror = alias\n";
        let c = cfg(text);
        assert_eq!(c.theme.error, ColorValue::Named(6));
        assert_eq!(warnings(text), Vec::<String>::new());
    }

    #[test]
    fn theme_key_reference() {
        let c = cfg("accent = #123456\npriority_marker = accent\n");
        assert_eq!(c.theme.priority_marker, ColorValue::Rgb(0x12, 0x34, 0x56));
    }

    #[test]
    fn unused_variable_warns_with_line_number() {
        let w = warnings("\nno_colour = true\n");
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("cfg:2:"), "{w:?}");
        assert!(w[0].contains("no_colour"));
    }

    #[test]
    fn forward_reference_is_a_literal_and_warns() {
        // `alias` is defined after use, so `error = alias` fails color parse.
        let w = warnings("error = alias\nalias = red\nwarning = alias\n");
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("cfg:1:"), "{w:?}");
    }

    #[test]
    fn missing_equals_and_bad_key_warn_and_skip() {
        let w = warnings("just some words\nBadKey = red\n");
        assert_eq!(w.len(), 2);
        assert!(w[0].starts_with("cfg:1:"));
        assert!(w[1].starts_with("cfg:2:"));
    }

    #[test]
    fn duplicate_key_last_wins() {
        let c = cfg("error = green\nerror = yellow\n");
        assert_eq!(c.theme.error, ColorValue::Named(3));
    }

    #[test]
    fn default_keyword_keeps_builtin() {
        let c = cfg("error = default\nrusk_db = default\nweb_port = default\n");
        assert_eq!(c, Config::default());
    }

    #[test]
    fn default_can_be_shadowed_by_a_variable() {
        let c = cfg("default = /tmp/x.json\nrusk_db = default\n");
        assert_eq!(c.rusk_db.unwrap(), "/tmp/x.json");
    }

    #[test]
    fn bools_are_strict() {
        let c = cfg("no_color = true\ncompact = true\nbackup = false\n");
        assert!(c.no_color && c.compact && !c.backup);
        let w = warnings("no_color = yes\n");
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("no_color"));
    }

    #[test]
    fn keywords_default_and_custom_lists() {
        assert_eq!(
            Config::default().keywords,
            vec!["TEMP", "INFO", "FIXME", "WIP"]
        );
        // Space- and comma-separated forms are equivalent.
        assert_eq!(cfg("keywords = BUG HACK\n").keywords, vec!["BUG", "HACK"]);
        assert_eq!(cfg("keywords = BUG, HACK\n").keywords, vec!["BUG", "HACK"]);
        // An empty value disables highlighting, silently (`""` works too);
        // other settings still reject empty values.
        let outcome = parse("keywords =\n");
        assert!(outcome.config.keywords.is_empty());
        assert!(outcome.warnings.is_empty());
        assert!(cfg("keywords = \"\"\n").keywords.is_empty());
        let w = warnings("web_token =\n");
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("empty value"));
    }

    #[test]
    fn invalid_color_warns_and_keeps_default() {
        let outcome = parse("error = blu\n");
        assert_eq!(outcome.config.theme.error, ColorValue::Named(1));
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("'error'"));
        assert!(outcome.warnings[0].contains("using default"));
    }

    #[test]
    fn web_and_sync_settings() {
        let c = cfg(
            "web_host = 0.0.0.0\nweb_port = 8080\nweb_token = s3cret\n\
             sync_remote = user@host:/tasks/tasks.json\nsync_token = tok\n",
        );
        assert_eq!(c.web_host, "0.0.0.0");
        assert_eq!(c.web_port, 8080);
        assert_eq!(c.web_token.as_deref(), Some("s3cret"));
        assert_eq!(c.sync_remote.as_deref(), Some("user@host:/tasks/tasks.json"));
        assert_eq!(c.sync_token.as_deref(), Some("tok"));
    }

    #[test]
    fn crlf_is_tolerated() {
        let c = cfg("error = green\r\ncompact = true\r\n");
        assert_eq!(c.theme.error, ColorValue::Named(2));
        assert!(c.compact);
    }

    #[test]
    fn empty_value_warns() {
        let w = warnings("web_token =\n");
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("empty value"));
    }

    #[test]
    fn rusk_db_is_kept_as_written() {
        // `~` is expanded where every location is read (crate::location),
        // for RUSK_DB as much as for rusk_db.
        let c = cfg("rusk_db = ~/tasks/db.json\n");
        assert_eq!(c.rusk_db.as_deref(), Some("~/tasks/db.json"));
    }

    #[test]
    fn remote_locations_are_kept_verbatim() {
        let c = cfg("rusk_db = https://tasks.example.com\ndb_token = tok\ngit_backend = true\n");
        assert_eq!(c.rusk_db.as_deref(), Some("https://tasks.example.com"));
        assert_eq!(c.db_token.as_deref(), Some("tok"));
        assert!(c.git_backend);
    }

    #[test]
    fn theme_entries_cover_all_keys() {
        let t = Theme::default();
        assert_eq!(t.entries().len(), Theme::KEYS.len());
        assert_eq!(t.entries().len(), 20);
    }
}
