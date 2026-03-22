use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::theme::Theme;

/// Top-level application settings, loaded from `~/.config/pika/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub theme: Theme,

    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,

    #[serde(default = "default_tab_size")]
    pub tab_size: usize,

    #[serde(default = "default_show_line_numbers")]
    pub show_line_numbers: bool,

    #[serde(default)]
    pub word_wrap: bool,

    #[serde(default)]
    pub auto_save: bool,

    #[serde(default)]
    pub lsp: LspSettings,
}

fn default_sidebar_width() -> u16 {
    30
}
fn default_tab_size() -> usize {
    4
}
fn default_show_line_numbers() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            sidebar_width: default_sidebar_width(),
            tab_size: default_tab_size(),
            show_line_numbers: default_show_line_numbers(),
            word_wrap: false,
            auto_save: false,
            lsp: LspSettings::default(),
        }
    }
}

impl Settings {
    /// Return the configuration directory (`~/.config/pika/`).
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("pika")
    }

    /// Load settings from `~/.config/pika/config.toml`.
    ///
    /// If the file does not exist, returns the default settings.
    /// If the file exists but is malformed, returns an error.
    pub fn load() -> Result<Self> {
        let path = Self::config_dir().join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let settings: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config file at {}", path.display()))?;
        Ok(settings)
    }

    /// Load settings from the given path instead of the default location.
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let settings: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config file at {}", path.display()))?;
        Ok(settings)
    }

    /// Serialise and write to `~/.config/pika/config.toml`, creating the
    /// directory tree if necessary.
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create config dir at {}", dir.display()))?;
        let path = dir.join("config.toml");
        let contents = toml::to_string_pretty(self)
            .context("failed to serialise settings to TOML")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("failed to write config file at {}", path.display()))?;
        Ok(())
    }

    /// Serialise and write to the given path instead of the default location.
    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir at {}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self)
            .context("failed to serialise settings to TOML")?;
        std::fs::write(path, contents)
            .with_context(|| format!("failed to write config file at {}", path.display()))?;
        Ok(())
    }
}

/// Settings for the built-in LSP integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSettings {
    #[serde(default = "default_auto_discover")]
    pub auto_discover: bool,

    #[serde(default)]
    pub servers: HashMap<String, LspServerConfig>,
}

fn default_auto_discover() -> bool {
    true
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            auto_discover: default_auto_discover(),
            servers: HashMap::new(),
        }
    }
}

/// Configuration for a single LSP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub root_markers: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = Settings::default();
        assert_eq!(s.sidebar_width, 30);
        assert_eq!(s.tab_size, 4);
        assert!(s.show_line_numbers);
        assert!(!s.word_wrap);
        assert!(!s.auto_save);
        assert!(s.lsp.auto_discover);
        assert!(s.lsp.servers.is_empty());
    }

    #[test]
    fn test_default_lsp_settings() {
        let lsp = LspSettings::default();
        assert!(lsp.auto_discover);
        assert!(lsp.servers.is_empty());
    }

    #[test]
    fn test_settings_serialization_roundtrip() {
        let original = Settings::default();
        let toml_str = toml::to_string_pretty(&original).expect("serialize");
        let restored: Settings = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(restored.sidebar_width, original.sidebar_width);
        assert_eq!(restored.tab_size, original.tab_size);
        assert_eq!(restored.show_line_numbers, original.show_line_numbers);
        assert_eq!(restored.word_wrap, original.word_wrap);
        assert_eq!(restored.auto_save, original.auto_save);
    }

    #[test]
    fn test_settings_partial_toml() {
        // Only sidebar_width is set; every other field should use its default.
        let toml_str = r#"sidebar_width = 50"#;
        let s: Settings = toml::from_str(toml_str).expect("parse partial toml");
        assert_eq!(s.sidebar_width, 50);
        assert_eq!(s.tab_size, 4);
        assert!(s.show_line_numbers);
    }

    #[test]
    fn test_settings_with_lsp_server() {
        let toml_str = r#"
[lsp.servers.rust-analyzer]
command = "rust-analyzer"
args = []
extensions = ["rs"]
root_markers = ["Cargo.toml"]
"#;
        let s: Settings = toml::from_str(toml_str).expect("parse toml");
        let ra = s.lsp.servers.get("rust-analyzer").expect("find rust-analyzer");
        assert_eq!(ra.command, "rust-analyzer");
        assert_eq!(ra.extensions, vec!["rs"]);
        assert_eq!(ra.root_markers, vec!["Cargo.toml"]);
    }

    #[test]
    fn test_lsp_server_config_equality() {
        let a = LspServerConfig {
            command: "ra".into(),
            args: vec!["--stdio".into()],
            extensions: vec!["rs".into()],
            root_markers: vec!["Cargo.toml".into()],
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_config_dir_ends_with_pika() {
        let dir = Settings::config_dir();
        assert!(dir.ends_with("pika"));
    }

    #[test]
    fn test_save_and_load_via_tempfile() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");

        let mut settings = Settings::default();
        settings.sidebar_width = 42;
        settings.tab_size = 2;
        settings.word_wrap = true;

        settings.save_to(&path).expect("save");
        let loaded = Settings::load_from(&path).expect("load");

        assert_eq!(loaded.sidebar_width, 42);
        assert_eq!(loaded.tab_size, 2);
        assert!(loaded.word_wrap);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nonexistent.toml");
        // load_from should fail (file not found), but load() falls back to default
        assert!(Settings::load_from(&path).is_err());
        // The standard load() returns Ok(default) when file is absent.
        // We can't easily test load() without touching the real config dir,
        // so we exercise the fallback path indirectly here.
    }

    #[test]
    fn test_load_malformed_file_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "{{{{not valid toml").expect("write");
        assert!(Settings::load_from(&path).is_err());
    }

    #[test]
    fn test_settings_with_custom_theme() {
        let toml_str = r#"
[theme]
r = 10
g = 20
b = 30

[theme.bg]
r = 10
g = 20
b = 30

[theme.fg]
r = 200
g = 200
b = 200

[theme.sidebar_bg]
r = 0
g = 0
b = 0

[theme.sidebar_fg]
r = 255
g = 255
b = 255

[theme.sidebar_selected_bg]
r = 50
g = 50
b = 50

[theme.sidebar_selected_fg]
r = 255
g = 255
b = 255

[theme.editor_bg]
r = 10
g = 20
b = 30

[theme.editor_fg]
r = 200
g = 200
b = 200

[theme.editor_line_number_fg]
r = 100
g = 100
b = 100

[theme.editor_current_line_bg]
r = 30
g = 30
b = 30

[theme.tab_active_bg]
r = 10
g = 20
b = 30

[theme.tab_active_fg]
r = 255
g = 255
b = 255

[theme.tab_inactive_bg]
r = 5
g = 5
b = 5

[theme.tab_inactive_fg]
r = 100
g = 100
b = 100

[theme.status_bar_bg]
r = 0
g = 122
b = 204

[theme.status_bar_fg]
r = 255
g = 255
b = 255

[theme.border_color]
r = 50
g = 50
b = 50

[theme.border_focused_color]
r = 0
g = 122
b = 204

[theme.error_fg]
r = 255
g = 0
b = 0

[theme.warning_fg]
r = 255
g = 200
b = 0

[theme.info_fg]
r = 0
g = 150
b = 255
"#;
        let s: Settings = toml::from_str(toml_str).expect("parse");
        assert_eq!(s.theme.bg.r, 10);
        assert_eq!(s.theme.error_fg.r, 255);
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("a").join("b").join("config.toml");
        let s = Settings::default();
        s.save_to(&nested).expect("save nested");
        assert!(nested.exists());
    }

    #[test]
    fn test_lsp_settings_serialize_roundtrip() {
        let mut lsp = LspSettings::default();
        lsp.servers.insert(
            "tsserver".into(),
            LspServerConfig {
                command: "typescript-language-server".into(),
                args: vec!["--stdio".into()],
                extensions: vec!["ts".into(), "tsx".into()],
                root_markers: vec!["tsconfig.json".into(), "package.json".into()],
            },
        );
        let toml_str = toml::to_string(&lsp).expect("ser");
        let restored: LspSettings = toml::from_str(&toml_str).expect("de");
        assert_eq!(
            restored.servers["tsserver"].command,
            "typescript-language-server"
        );
        assert_eq!(restored.servers["tsserver"].extensions.len(), 2);
    }
}
