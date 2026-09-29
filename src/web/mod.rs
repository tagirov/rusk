//! Web frontend shared by `rusk gen` (static, read-only) and `rusk serve`
//! (live, JSON API). One mobile-first single-file template, no build step:
//! placeholders are substituted with plain `String::replace`.

pub mod api;
pub mod server;

use crate::Task;
use crate::config::Theme;
use anyhow::{Context, Result, ensure};

const TEMPLATE: &str = include_str!("template.html");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Static,
    Live,
}

/// CSS custom properties from the theme: `--rusk-<key>: <hex>;` with
/// underscores turned into hyphens. Terminal-default colors are skipped
/// (the template's fallback values win).
pub fn theme_css(theme: &Theme) -> String {
    let mut css = String::new();
    for (key, color) in theme.entries() {
        if let Some(hex) = color.to_css_hex() {
            css.push_str(&format!("--rusk-{}: {hex}; ", key.replace('_', "-")));
        }
    }
    css.trim_end().to_string()
}

/// Task list as JSON safe for inlining into a `<script>` block: `<` can only
/// occur inside JSON string literals, and escaping it neutralizes
/// `</script>` and HTML-comment injection from task text. The line
/// separators U+2028 and U+2029 are JSON's to leave as they are but were
/// line breaks to JavaScript before ES2019, where they end the string
/// literal (SECURITY.md L2): escaped as well, they are a task's text in
/// every engine.
fn tasks_json(tasks: &[Task]) -> Result<String> {
    let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
    Ok(json
        .replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

/// `signed_in`: the page is served behind a token, and offers to sign out.
pub fn render_template(
    mode: Mode,
    theme_css: &str,
    tasks: &[Task],
    generated: &str,
    signed_in: bool,
) -> Result<String> {
    let mode_str = match mode {
        Mode::Static => "static",
        Mode::Live => "live",
    };
    // The live page asks for no icon: `rusk serve` has none, and a browser
    // would ask for `/favicon.ico` on every load. A static page is put on
    // a site of its own, whose icon it keeps (review of R29).
    let icon = match mode {
        Mode::Static => "",
        Mode::Live => r#"<link rel="icon" href="data:,">"#,
    };
    let html = TEMPLATE
        .replacen("<!--__ICON__-->", icon, 1)
        .replacen("__MODE__", mode_str, 1)
        .replacen("/*__THEME__*/", theme_css, 1)
        .replacen("__GENERATED__", generated, 1)
        .replacen("__SIGNED_IN__", if signed_in { "true" } else { "false" }, 1);
    // Validate before injecting task data, which may legitimately contain
    // marker-looking text.
    ensure!(
        !html.contains("__MODE__")
            && !html.contains("__GENERATED__")
            && !html.contains("__SIGNED_IN__")
            && !html.contains("__ICON__"),
        "template markers were not replaced"
    );
    ensure!(
        html.contains("/*__DATA__*/[]"),
        "template data marker is missing"
    );
    Ok(html.replacen("/*__DATA__*/[]", &tasks_json(tasks)?, 1))
}

/// Static page for `rusk gen`: data embedded, stamped with local time.
pub fn render_static_page(tasks: &[Task]) -> Result<String> {
    let generated = chrono::Local::now().format("%d-%m-%Y %H:%M").to_string();
    render_template(
        Mode::Static,
        &theme_css(&crate::config::config().theme),
        tasks,
        &generated,
        false,
    )
}

/// Live page for `rusk serve`: tasks inlined for instant first paint,
/// then the client re-fetches from the API.
pub fn render_live_page(tasks: &[Task], signed_in: bool) -> Result<String> {
    render_template(
        Mode::Live,
        &theme_css(&crate::config::config().theme),
        tasks,
        "",
        signed_in,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ColorValue, Theme};

    fn task(text: &str) -> Task {
        Task {
            id: 1,
            text: text.to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 7, 8),
            done: false,
            priority: true, after: Vec::new(),
        }
    }

    #[test]
    fn all_markers_are_replaced() {
        let html =
            render_template(Mode::Static, "--rusk-accent: #ffa500;", &[task("hello")], "now", false)
                .unwrap();
        for marker in ["__MODE__", "__THEME__", "__DATA__", "__GENERATED__"] {
            assert!(!html.contains(marker), "marker {marker} left in output");
        }
        assert!(html.contains(r#"mode: "static""#));
        assert!(html.contains("--rusk-accent: #ffa500;"));
        assert!(html.contains("hello"));
        assert!(html.contains(r#"generated: "now""#));
    }

    #[test]
    fn live_mode_marker() {
        let html = render_template(Mode::Live, "", &[], "", true).unwrap();
        assert!(html.contains(r#"mode: "live""#));
        assert!(html.contains("tasks: []"));
    }

    #[test]
    fn script_injection_is_neutralized() {
        let evil = "</script><script>alert(1)</script>";
        let html = render_template(Mode::Static, "", &[task(evil)], "", false).unwrap();
        assert!(!html.contains(evil), "raw </script> from task text must not appear");
        assert!(html.contains("\\u003c/script>\\u003cscript>"));

        // SECURITY.md L2: a line separator is a character of the text,
        // not the end of the string literal — in old engines too.
        let html = render_template(Mode::Static, "", &[task("a\u{2028}b\u{2029}c")], "", false).unwrap();
        assert!(!html.contains('\u{2028}') && !html.contains('\u{2029}'));
        assert!(html.contains("a\\u2028b\\u2029c"), "{html}");
    }

    #[test]
    fn theme_css_maps_keys_and_skips_default() {
        let theme = Theme {
            task_id: ColorValue::Default,
            ..Theme::default()
        };
        let css = theme_css(&theme);
        assert!(css.contains("--rusk-priority-marker: #ffa500;"));
        assert!(css.contains("--rusk-date-overdue: #cd0000;"));
        assert!(!css.contains("--rusk-task-id"), "Default colors are skipped");
    }
}
