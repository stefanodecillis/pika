use serde::{Deserialize, Serialize};

/// An RGB color used throughout the UI theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to a ratatui `Color::Rgb`.
    pub fn to_ratatui_color(self) -> ratatui::style::Color {
       ratatui::style::Color::Rgb(self.r, self.g, self.b)
    }
}

/// Full colour theme for the Pika terminal IDE.
///
/// The default is a dark palette inspired by VS Code Dark+ / base16-ocean-dark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    // -- overall defaults --
    pub bg: Color,
    pub fg: Color,

    // -- sidebar --
    pub sidebar_bg: Color,
    pub sidebar_fg: Color,
    pub sidebar_selected_bg: Color,
    pub sidebar_selected_fg: Color,

    // -- editor --
    pub editor_bg: Color,
    pub editor_fg: Color,
    pub editor_line_number_fg: Color,
    pub editor_current_line_bg: Color,

    // -- tabs --
    pub tab_active_bg: Color,
    pub tab_active_fg: Color,
    pub tab_inactive_bg: Color,
    pub tab_inactive_fg: Color,

    // -- status bar --
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,

    // -- borders --
    pub border_color: Color,
    pub border_focused_color: Color,

    // -- diagnostics --
    pub error_fg: Color,
    pub warning_fg: Color,
    pub info_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Base16 Ocean Dark inspired defaults
            bg: Color::new(0x2b, 0x30, 0x3b),              // #2b303b
            fg: Color::new(0xc0, 0xc5, 0xce),              // #c0c5ce

            sidebar_bg: Color::new(0x25, 0x2a, 0x34),      // slightly darker
            sidebar_fg: Color::new(0xc0, 0xc5, 0xce),
            sidebar_selected_bg: Color::new(0x4f, 0x56, 0x67), // #4f5667
            sidebar_selected_fg: Color::new(0xff, 0xff, 0xff),

            editor_bg: Color::new(0x2b, 0x30, 0x3b),
            editor_fg: Color::new(0xc0, 0xc5, 0xce),
            editor_line_number_fg: Color::new(0x65, 0x73, 0x7e), // #65737e
            editor_current_line_bg: Color::new(0x34, 0x3d, 0x46), // #343d46

            tab_active_bg: Color::new(0x2b, 0x30, 0x3b),
            tab_active_fg: Color::new(0xff, 0xff, 0xff),
            tab_inactive_bg: Color::new(0x1b, 0x20, 0x2a),
            tab_inactive_fg: Color::new(0x65, 0x73, 0x7e),

            status_bar_bg: Color::new(0x00, 0x7a, 0xcc),   // VS Code blue
            status_bar_fg: Color::new(0xff, 0xff, 0xff),

            border_color: Color::new(0x3b, 0x41, 0x4d),
            border_focused_color: Color::new(0x00, 0x7a, 0xcc),

            error_fg: Color::new(0xbf, 0x61, 0x6a),        // #bf616a
            warning_fg: Color::new(0xeb, 0xcb, 0x8b),      // #ebcb8b
            info_fg: Color::new(0x8f, 0xa1, 0xb3),         // #8fa1b3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_new() {
        let c = Color::new(10, 20, 30);
        assert_eq!(c.r, 10);
        assert_eq!(c.g, 20);
        assert_eq!(c.b, 30);
    }

    #[test]
    fn test_color_to_ratatui() {
        let c = Color::new(0xAB, 0xCD, 0xEF);
        let rc = c.to_ratatui_color();
        assert_eq!(rc, ratatui::style::Color::Rgb(0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn test_color_equality() {
        let a = Color::new(1, 2, 3);
        let b = Color::new(1, 2, 3);
        let c = Color::new(4, 5, 6);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_color_clone() {
        let a = Color::new(100, 200, 50);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_color_debug() {
        let c = Color::new(0, 0, 0);
        let dbg = format!("{:?}", c);
        assert!(dbg.contains("Color"));
    }

    #[test]
    fn test_theme_default_creates_valid_theme() {
        let theme = Theme::default();
        // Spot-check a handful of known values
        assert_eq!(theme.bg, Color::new(0x2b, 0x30, 0x3b));
        assert_eq!(theme.fg, Color::new(0xc0, 0xc5, 0xce));
        assert_eq!(theme.status_bar_bg, Color::new(0x00, 0x7a, 0xcc));
        assert_eq!(theme.error_fg, Color::new(0xbf, 0x61, 0x6a));
    }

    #[test]
    fn test_theme_clone() {
        let t1 = Theme::default();
        let t2 = t1.clone();
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_theme_serialization_roundtrip() {
        let original = Theme::default();
        let toml_str = toml::to_string(&original).expect("serialize theme");
        let restored: Theme = toml::from_str(&toml_str).expect("deserialize theme");
        assert_eq!(original, restored);
    }

    #[test]
    fn test_color_serialization_roundtrip() {
        let color = Color::new(42, 128, 255);
        let toml_str = toml::to_string(&color).expect("serialize color");
        let restored: Color = toml::from_str(&toml_str).expect("deserialize color");
        assert_eq!(color, restored);
    }

    #[test]
    fn test_theme_partial_deserialization() {
        // If we only supply some fields plus defaults, the toml round-trip
        // still works provided every field is present.
        let theme = Theme::default();
        let serialized = toml::to_string(&theme).unwrap();
        assert!(serialized.contains("bg"));
        assert!(serialized.contains("error_fg"));
    }

    #[test]
    fn test_all_theme_colors_convert_to_ratatui() {
        let t = Theme::default();
        // Just make sure no panic happens
        let _ = t.bg.to_ratatui_color();
        let _ = t.fg.to_ratatui_color();
        let _ = t.sidebar_bg.to_ratatui_color();
        let _ = t.sidebar_fg.to_ratatui_color();
        let _ = t.sidebar_selected_bg.to_ratatui_color();
        let _ = t.sidebar_selected_fg.to_ratatui_color();
        let _ = t.editor_bg.to_ratatui_color();
        let _ = t.editor_fg.to_ratatui_color();
        let _ = t.editor_line_number_fg.to_ratatui_color();
        let _ = t.editor_current_line_bg.to_ratatui_color();
        let _ = t.tab_active_bg.to_ratatui_color();
        let _ = t.tab_active_fg.to_ratatui_color();
        let _ = t.tab_inactive_bg.to_ratatui_color();
        let _ = t.tab_inactive_fg.to_ratatui_color();
        let _ = t.status_bar_bg.to_ratatui_color();
        let _ = t.status_bar_fg.to_ratatui_color();
        let _ = t.border_color.to_ratatui_color();
        let _ = t.border_focused_color.to_ratatui_color();
        let _ = t.error_fg.to_ratatui_color();
        let _ = t.warning_fg.to_ratatui_color();
        let _ = t.info_fg.to_ratatui_color();
    }

    #[test]
    fn test_color_boundary_values() {
        let black = Color::new(0, 0, 0);
        assert_eq!(black.to_ratatui_color(), ratatui::style::Color::Rgb(0, 0, 0));

        let white = Color::new(255, 255, 255);
        assert_eq!(
            white.to_ratatui_color(),
            ratatui::style::Color::Rgb(255, 255, 255)
        );
    }
}
