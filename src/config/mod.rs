use anyhow::{Context, Result};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Configuration for issue tracker integrations (JIRA, Linear, GitHub, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub integrations: IntegrationConfig,

    #[serde(default)]
    pub theme: ThemeConfig,

    #[serde(default)]
    pub columns: ColumnVisibilityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationConfig {
    /// Default tracker name when auto-detection is ambiguous
    #[serde(default)]
    pub default_tracker: Option<String>,

    /// Map of tracker name to tracker configuration
    #[serde(default)]
    pub trackers: HashMap<String, TrackerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    /// Regex patterns to match ticket IDs for this tracker
    #[serde(default)]
    pub ticket_patterns: Vec<String>,
    /// URL template for browsing tickets: {base_url}, {ticket}
    #[serde(default)]
    pub browse_url: String,
    /// URL template for worklog page: {base_url}, {ticket}
    #[serde(default)]
    pub worklog_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnVisibilityConfig {
    #[serde(default = "default_show_project")]
    pub project: bool,
    #[serde(default = "default_show_customer")]
    pub customer: bool,
    #[serde(default = "default_show_description")]
    pub description: bool,
}

fn default_show_project() -> bool {
    true
}

fn default_show_customer() -> bool {
    false
}

fn default_show_description() -> bool {
    true
}

impl Default for ColumnVisibilityConfig {
    fn default() -> Self {
        Self {
            project: default_show_project(),
            customer: default_show_customer(),
            description: default_show_description(),
        }
    }
}

impl Config {
    /// Load config from file, or return defaults if file doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path();

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .context(format!("Failed to read config file: {:?}", config_path))?;
            let config: Config =
                toml::from_str(&contents).context("Failed to parse config TOML")?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Get config file path (~/.config/work-tuimer/config.toml)
    /// Respects XDG_CONFIG_HOME environment variable on Unix systems
    fn get_config_path() -> PathBuf {
        // On Unix systems (Linux/macOS), respect XDG_CONFIG_HOME
        #[cfg(unix)]
        {
            if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
                return PathBuf::from(xdg_config)
                    .join("work-tuimer")
                    .join("config.toml");
            }
            // Fall back to ~/.config if XDG_CONFIG_HOME is not set
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home)
                    .join(".config")
                    .join("work-tuimer")
                    .join("config.toml");
            }
        }

        // On Windows, use dirs::config_dir() which returns AppData/Roaming
        #[cfg(windows)]
        {
            if let Some(config_dir) = dirs::config_dir() {
                return config_dir.join("work-tuimer").join("config.toml");
            }
        }

        // Final fallback for any platform
        PathBuf::from("./config.toml")
    }

    /// Check if any tracker integration is properly configured
    pub fn has_integrations(&self) -> bool {
        self.integrations
            .trackers
            .values()
            .any(|tracker| tracker.enabled && !tracker.base_url.is_empty())
    }

    /// Get the active theme (either pre-defined or custom)
    pub fn get_theme(&self) -> Theme {
        self.theme.get_active_theme()
    }
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Name of active theme: "default", "kanagawa", "catppuccin", "gruvbox", "monokai", "dracula", "everforest", "terminal"
    #[serde(default = "default_theme_name")]
    pub active: String,

    /// Custom theme definitions
    #[serde(default)]
    pub custom: HashMap<String, CustomThemeColors>,
}

fn default_theme_name() -> String {
    "default".to_string()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            active: default_theme_name(),
            custom: HashMap::new(),
        }
    }
}

impl ThemeConfig {
    /// Get the active theme based on config
    pub fn get_active_theme(&self) -> Theme {
        // Check custom themes first (allows overriding predefined themes)
        if let Some(custom_colors) = self.custom.get(&self.active) {
            return Theme::from_custom(custom_colors);
        }

        // Then check if it's a pre-defined theme
        match self.active.as_str() {
            "default" => Theme::default_theme(),
            "kanagawa" => Theme::kanagawa(),
            "catppuccin" => Theme::catppuccin(),
            "gruvbox" => Theme::gruvbox(),
            "monokai" => Theme::monokai(),
            "dracula" => Theme::dracula(),
            "everforest" => Theme::everforest(),
            "terminal" => Theme::terminal(),
            _ => {
                // Fallback to default if theme not found
                Theme::default_theme()
            }
        }
    }
}

/// Custom theme color definitions (supports hex colors, RGB tuples, and named colors)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemeColors {
    // Border colors
    pub active_border: String,
    pub inactive_border: String,
    pub searching_border: String,

    // Background colors
    pub selected_bg: String,
    pub selected_inactive_bg: String,
    pub visual_bg: String,
    pub timer_active_bg: String,
    pub row_alternate_bg: String,
    pub edit_bg: String,
    pub focus_bg: String,

    // Text colors
    pub primary_text: String,
    pub secondary_text: String,
    pub highlight_text: String,

    // Status colors
    pub success: String,
    pub warning: String,
    pub error: String,
    pub info: String,

    // Specific element colors
    pub timer_text: String,
    pub badge: String,
}

/// Theme with semantic color names (resolved to ratatui Colors)
#[derive(Debug, Clone)]
pub struct Theme {
    // Border colors
    pub active_border: Color,
    #[allow(dead_code)]
    pub inactive_border: Color,
    #[allow(dead_code)]
    pub searching_border: Color,

    // Background colors
    pub selected_bg: Color,
    #[allow(dead_code)]
    pub selected_inactive_bg: Color,
    pub visual_bg: Color,
    pub timer_active_bg: Color,
    pub row_alternate_bg: Color,
    pub edit_bg: Color,
    pub focus_bg: Color,

    // Text colors
    pub primary_text: Color,
    pub secondary_text: Color,
    pub highlight_text: Color,

    // Status colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // Specific element colors
    pub timer_text: Color,
    pub badge: Color,
}

impl Theme {
    /// Default theme (current color scheme)
    pub fn default_theme() -> Self {
        Self {
            active_border: Color::Cyan,
            inactive_border: Color::DarkGray,
            searching_border: Color::Yellow,
            selected_bg: Color::Rgb(40, 40, 60),
            selected_inactive_bg: Color::Rgb(30, 30, 45),
            visual_bg: Color::Rgb(70, 130, 180),
            timer_active_bg: Color::Rgb(34, 139, 34),
            row_alternate_bg: Color::Rgb(25, 25, 35),
            edit_bg: Color::Rgb(22, 78, 99),
            focus_bg: Color::Rgb(80, 60, 120), // purple focus indicator
            primary_text: Color::White,
            secondary_text: Color::Gray,
            highlight_text: Color::Cyan,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::LightRed,
            info: Color::Cyan,
            timer_text: Color::Yellow,
            badge: Color::LightMagenta,
        }
    }

    /// Kanagawa theme (dark navy blue aesthetic inspired by Hokusai's painting)
    pub fn kanagawa() -> Self {
        Self {
            active_border: Color::Rgb(127, 180, 202), // springBlue (brighter)
            inactive_border: Color::Rgb(54, 59, 77),  // sumiInk4
            searching_border: Color::Rgb(255, 160, 102), // surimiOrange
            selected_bg: Color::Rgb(42, 42, 55),      // sumiInk3 (slightly lighter for distinction)
            selected_inactive_bg: Color::Rgb(31, 31, 40), // sumiInk1/2
            visual_bg: Color::Rgb(45, 79, 103),       // waveBlue2 (darker for distinction)
            timer_active_bg: Color::Rgb(152, 187, 106), // springGreen (brighter)
            row_alternate_bg: Color::Rgb(22, 25, 32), // sumiInk1 (darker)
            edit_bg: Color::Rgb(106, 149, 137),       // waveAqua1 (distinct teal, not blue!)
            focus_bg: Color::Rgb(149, 127, 184),      // oniViolet (purple focus indicator)
            primary_text: Color::Rgb(220, 215, 186),  // fujiWhite
            secondary_text: Color::Rgb(114, 118, 129), // fujiGray (comments)
            highlight_text: Color::Rgb(230, 195, 132), // carpYellow (warmer highlight)
            success: Color::Rgb(152, 187, 106),       // springGreen
            warning: Color::Rgb(255, 160, 102),       // surimiOrange
            error: Color::Rgb(255, 93, 98),           // peachRed (brighter)
            info: Color::Rgb(127, 180, 202),          // springBlue
            timer_text: Color::Rgb(230, 195, 132),    // carpYellow
            badge: Color::Rgb(149, 127, 184),         // oniViolet
        }
    }

    /// Catppuccin Mocha theme (popular pastel theme)
    pub fn catppuccin() -> Self {
        Self {
            active_border: Color::Rgb(137, 180, 250),     // blue
            inactive_border: Color::Rgb(69, 71, 90),      // surface1
            searching_border: Color::Rgb(249, 226, 175),  // yellow
            selected_bg: Color::Rgb(49, 50, 68),          // surface0
            selected_inactive_bg: Color::Rgb(30, 30, 46), // base
            visual_bg: Color::Rgb(116, 199, 236),         // sapphire
            timer_active_bg: Color::Rgb(166, 227, 161),   // green
            row_alternate_bg: Color::Rgb(24, 24, 37),     // mantle
            edit_bg: Color::Rgb(137, 180, 250),           // blue (dimmed)
            focus_bg: Color::Rgb(203, 166, 247),          // mauve (purple focus indicator)
            primary_text: Color::Rgb(205, 214, 244),      // text
            secondary_text: Color::Rgb(127, 132, 156),    // overlay0
            highlight_text: Color::Rgb(137, 180, 250),    // blue
            success: Color::Rgb(166, 227, 161),           // green
            warning: Color::Rgb(249, 226, 175),           // yellow
            error: Color::Rgb(243, 139, 168),             // red
            info: Color::Rgb(137, 180, 250),              // blue
            timer_text: Color::Rgb(245, 194, 231),        // pink
            badge: Color::Rgb(203, 166, 247),             // mauve
        }
    }

    /// Gruvbox theme (retro warm colors)
    pub fn gruvbox() -> Self {
        Self {
            active_border: Color::Rgb(131, 165, 152),     // aqua
            inactive_border: Color::Rgb(60, 56, 54),      // bg2
            searching_border: Color::Rgb(250, 189, 47),   // yellow
            selected_bg: Color::Rgb(60, 56, 54),          // bg2
            selected_inactive_bg: Color::Rgb(40, 40, 40), // bg0
            visual_bg: Color::Rgb(69, 133, 136),          // blue
            timer_active_bg: Color::Rgb(152, 151, 26),    // green
            row_alternate_bg: Color::Rgb(29, 32, 33),     // bg0_h
            edit_bg: Color::Rgb(80, 73, 69),              // bg3
            focus_bg: Color::Rgb(211, 134, 155),          // purple (warm purple focus)
            primary_text: Color::Rgb(235, 219, 178),      // fg
            secondary_text: Color::Rgb(146, 131, 116),    // fg4
            highlight_text: Color::Rgb(131, 165, 152),    // aqua
            success: Color::Rgb(184, 187, 38),            // green
            warning: Color::Rgb(250, 189, 47),            // yellow
            error: Color::Rgb(251, 73, 52),               // red
            info: Color::Rgb(131, 165, 152),              // aqua
            timer_text: Color::Rgb(254, 128, 25),         // orange
            badge: Color::Rgb(211, 134, 155),             // purple
        }
    }

    /// Monokai theme (classic vibrant)
    pub fn monokai() -> Self {
        Self {
            active_border: Color::Rgb(102, 217, 239), // cyan (vibrant)
            inactive_border: Color::Rgb(73, 72, 62),  // darker gray
            searching_border: Color::Rgb(230, 219, 116), // yellow
            selected_bg: Color::Rgb(73, 72, 62),      // dark gray
            selected_inactive_bg: Color::Rgb(39, 40, 34), // background
            visual_bg: Color::Rgb(249, 38, 114),      // magenta (distinct!)
            timer_active_bg: Color::Rgb(166, 226, 46), // vibrant green
            row_alternate_bg: Color::Rgb(30, 31, 28), // darker bg
            edit_bg: Color::Rgb(100, 85, 60),         // warm brown (distinct from selected_bg!)
            focus_bg: Color::Rgb(174, 129, 255),      // purple (vibrant purple focus)
            primary_text: Color::Rgb(248, 248, 242),  // foreground
            secondary_text: Color::Rgb(117, 113, 94), // comment gray
            highlight_text: Color::Rgb(253, 151, 31), // bright orange (not cyan!)
            success: Color::Rgb(166, 226, 46),        // vibrant green
            warning: Color::Rgb(230, 219, 116),       // yellow
            error: Color::Rgb(249, 38, 114),          // magenta/pink
            info: Color::Rgb(102, 217, 239),          // cyan
            timer_text: Color::Rgb(253, 151, 31),     // orange
            badge: Color::Rgb(174, 129, 255),         // purple
        }
    }

    /// Dracula theme (purple/pink accents)
    pub fn dracula() -> Self {
        Self {
            active_border: Color::Rgb(139, 233, 253),     // cyan
            inactive_border: Color::Rgb(68, 71, 90),      // current line
            searching_border: Color::Rgb(241, 250, 140),  // yellow
            selected_bg: Color::Rgb(68, 71, 90),          // current line
            selected_inactive_bg: Color::Rgb(40, 42, 54), // background
            visual_bg: Color::Rgb(139, 233, 253),         // cyan (distinct from purple)
            timer_active_bg: Color::Rgb(80, 250, 123),    // green
            row_alternate_bg: Color::Rgb(30, 31, 40),     // darker than bg
            edit_bg: Color::Rgb(98, 114, 164),            // purple (distinct from selected_bg!)
            focus_bg: Color::Rgb(189, 147, 249),          // bright purple (focus indicator)
            primary_text: Color::Rgb(248, 248, 242),      // foreground
            secondary_text: Color::Rgb(98, 114, 164),     // purple/comment
            highlight_text: Color::Rgb(189, 147, 249),    // bright purple
            success: Color::Rgb(80, 250, 123),            // green
            warning: Color::Rgb(241, 250, 140),           // yellow
            error: Color::Rgb(255, 85, 85),               // red
            info: Color::Rgb(139, 233, 253),              // cyan
            timer_text: Color::Rgb(255, 184, 108),        // orange
            badge: Color::Rgb(255, 121, 198),             // pink
        }
    }

    /// Everforest theme (green forest aesthetic)
    pub fn everforest() -> Self {
        Self {
            active_border: Color::Rgb(131, 192, 146),     // green
            inactive_border: Color::Rgb(83, 86, 77),      // bg3
            searching_border: Color::Rgb(219, 188, 127),  // yellow
            selected_bg: Color::Rgb(67, 72, 60),          // bg2
            selected_inactive_bg: Color::Rgb(45, 49, 41), // bg1
            visual_bg: Color::Rgb(123, 175, 153),         // aqua
            timer_active_bg: Color::Rgb(131, 192, 146),   // green
            row_alternate_bg: Color::Rgb(35, 38, 32),     // bg0
            edit_bg: Color::Rgb(83, 86, 77),              // bg3
            focus_bg: Color::Rgb(217, 143, 172),          // purple (soft purple focus)
            primary_text: Color::Rgb(211, 198, 170),      // fg
            secondary_text: Color::Rgb(146, 142, 123),    // gray1
            highlight_text: Color::Rgb(123, 175, 153),    // aqua
            success: Color::Rgb(131, 192, 146),           // green
            warning: Color::Rgb(219, 188, 127),           // yellow
            error: Color::Rgb(230, 126, 128),             // red
            info: Color::Rgb(123, 175, 153),              // aqua
            timer_text: Color::Rgb(230, 152, 117),        // orange
            badge: Color::Rgb(217, 143, 172),             // purple
        }
    }

    /// Terminal theme (uses terminal's native colors)
    pub fn terminal() -> Self {
        Self {
            active_border: Color::Cyan,
            inactive_border: Color::Reset,
            searching_border: Color::Yellow,
            selected_bg: Color::Blue,
            selected_inactive_bg: Color::Reset,
            visual_bg: Color::Blue,
            timer_active_bg: Color::Green,
            row_alternate_bg: Color::Reset,
            edit_bg: Color::Cyan,
            focus_bg: Color::Magenta, // magenta/purple focus
            primary_text: Color::Reset,
            secondary_text: Color::DarkGray,
            highlight_text: Color::Cyan,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Cyan,
            timer_text: Color::Yellow,
            badge: Color::Magenta,
        }
    }

    /// Create theme from custom color definitions
    pub fn from_custom(colors: &CustomThemeColors) -> Self {
        Self {
            active_border: parse_color(&colors.active_border),
            inactive_border: parse_color(&colors.inactive_border),
            searching_border: parse_color(&colors.searching_border),
            selected_bg: parse_color(&colors.selected_bg),
            selected_inactive_bg: parse_color(&colors.selected_inactive_bg),
            visual_bg: parse_color(&colors.visual_bg),
            timer_active_bg: parse_color(&colors.timer_active_bg),
            row_alternate_bg: parse_color(&colors.row_alternate_bg),
            edit_bg: parse_color(&colors.edit_bg),
            focus_bg: parse_color(&colors.focus_bg),
            primary_text: parse_color(&colors.primary_text),
            secondary_text: parse_color(&colors.secondary_text),
            highlight_text: parse_color(&colors.highlight_text),
            success: parse_color(&colors.success),
            warning: parse_color(&colors.warning),
            error: parse_color(&colors.error),
            info: parse_color(&colors.info),
            timer_text: parse_color(&colors.timer_text),
            badge: parse_color(&colors.badge),
        }
    }
}

/// Parse color string (supports hex, RGB tuples, and named colors)
fn parse_color(color_str: &str) -> Color {
    let trimmed = color_str.trim();

    // Handle hex colors (#RRGGBB or #RGB)
    if let Some(hex) = trimmed.strip_prefix('#') {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        } else if hex.len() == 3 {
            // Short hex format #RGB -> #RRGGBB
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..1].repeat(2), 16),
                u8::from_str_radix(&hex[1..2].repeat(2), 16),
                u8::from_str_radix(&hex[2..3].repeat(2), 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
    }

    // Handle named colors
    match trimmed.to_lowercase().as_str() {
        "reset" | "terminal" | "default" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        _ => {
            // Fallback: try to parse as RGB tuple "r,g,b" or "(r, g, b)"
            // Strip parentheses if present
            let rgb_str = trimmed.trim_start_matches('(').trim_end_matches(')').trim();
            let parts: Vec<&str> = rgb_str.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3
                && let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                )
            {
                return Color::Rgb(r, g, b);
            }
            // Final fallback: return default white
            Color::White
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.integrations.default_tracker, None);
        assert!(config.integrations.trackers.is_empty());
        assert!(config.columns.project);
        assert!(!config.columns.customer);
        assert!(config.columns.description);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).expect("Failed to serialize");
        assert!(toml_str.contains("integrations"));
        assert!(toml_str.contains("columns"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
[integrations]
default_tracker = "my-jira"

[integrations.trackers.my-jira]
enabled = true
base_url = "https://test.atlassian.net"
ticket_patterns = ["^PROJ-\\d+$"]
browse_url = "{base_url}/browse/{ticket}"
worklog_url = "{base_url}/browse/{ticket}?focusedWorklogId=-1"
        "#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(
            config.integrations.default_tracker,
            Some("my-jira".to_string())
        );
        let tracker = config.integrations.trackers.get("my-jira").unwrap();
        assert_eq!(tracker.base_url, "https://test.atlassian.net");
        assert_eq!(tracker.ticket_patterns[0], "^PROJ-\\d+$");
        assert!(config.columns.project);
        assert!(!config.columns.customer);
        assert!(config.columns.description);
    }

    #[test]
    fn test_columns_config_deserialization() {
        let toml_str = r#"
[columns]
project = false
customer = true
description = false
        "#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize");
        assert!(!config.columns.project);
        assert!(config.columns.customer);
        assert!(!config.columns.description);
    }

    #[test]
    fn test_columns_config_partial_defaults() {
        let toml_str = r#"
[columns]
customer = true
        "#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize");
        assert!(config.columns.project);
        assert!(config.columns.customer);
        assert!(config.columns.description);
    }

    #[test]
    fn test_tracker_config_defaults() {
        let tracker = TrackerConfig::default();
        assert!(!tracker.enabled);
        assert!(tracker.base_url.is_empty());
        assert!(tracker.ticket_patterns.is_empty());
    }

    // Theme-related tests

    #[test]
    fn test_default_theme_config() {
        let theme_config = ThemeConfig::default();
        assert_eq!(theme_config.active, "default");
        assert!(theme_config.custom.is_empty());
    }

    #[test]
    fn test_theme_config_get_default_theme() {
        let theme_config = ThemeConfig::default();
        let theme = theme_config.get_active_theme();
        // Verify it returns a valid theme (checking a few fields)
        assert!(matches!(theme.active_border, Color::Cyan));
        assert!(matches!(theme.error, Color::LightRed));
    }

    #[test]
    fn test_theme_config_get_kanagawa_theme() {
        let theme_config = ThemeConfig {
            active: "kanagawa".to_string(),
            custom: HashMap::new(),
        };
        let theme = theme_config.get_active_theme();
        // Verify it returns kanagawa theme (check one specific color)
        assert!(matches!(theme.active_border, Color::Rgb(127, 180, 202)));
    }

    #[test]
    fn test_theme_config_get_all_predefined_themes() {
        let theme_names = vec![
            "default",
            "kanagawa",
            "catppuccin",
            "gruvbox",
            "monokai",
            "dracula",
            "everforest",
            "terminal",
        ];

        for name in theme_names {
            let theme_config = ThemeConfig {
                active: name.to_string(),
                custom: HashMap::new(),
            };
            let _theme = theme_config.get_active_theme();
            // Just verify it doesn't panic and returns a theme
        }
    }

    #[test]
    fn test_parse_color_hex_6_digit() {
        let color = parse_color("#7e9cd8");
        assert!(matches!(color, Color::Rgb(126, 156, 216)));

        let color = parse_color("#FF0000");
        assert!(matches!(color, Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_color_hex_3_digit() {
        let color = parse_color("#F00");
        assert!(matches!(color, Color::Rgb(255, 0, 0)));

        let color = parse_color("#0F0");
        assert!(matches!(color, Color::Rgb(0, 255, 0)));

        let color = parse_color("#00F");
        assert!(matches!(color, Color::Rgb(0, 0, 255)));
    }

    #[test]
    fn test_parse_color_rgb_tuple() {
        let color = parse_color("255, 128, 64");
        assert!(matches!(color, Color::Rgb(255, 128, 64)));

        let color = parse_color("0,255,0");
        assert!(matches!(color, Color::Rgb(0, 255, 0)));

        let color = parse_color(" 100 , 200 , 150 ");
        assert!(matches!(color, Color::Rgb(100, 200, 150)));
    }

    #[test]
    fn test_parse_color_named_colors() {
        assert!(matches!(parse_color("red"), Color::Red));
        assert!(matches!(parse_color("Red"), Color::Red));
        assert!(matches!(parse_color("RED"), Color::Red));
        assert!(matches!(parse_color("green"), Color::Green));
        assert!(matches!(parse_color("blue"), Color::Blue));
        assert!(matches!(parse_color("yellow"), Color::Yellow));
        assert!(matches!(parse_color("cyan"), Color::Cyan));
        assert!(matches!(parse_color("magenta"), Color::Magenta));
        assert!(matches!(parse_color("white"), Color::White));
        assert!(matches!(parse_color("black"), Color::Black));
        assert!(matches!(parse_color("gray"), Color::Gray));
        assert!(matches!(parse_color("grey"), Color::Gray));
        assert!(matches!(parse_color("darkgray"), Color::DarkGray));
        assert!(matches!(parse_color("lightred"), Color::LightRed));
        assert!(matches!(parse_color("lightgreen"), Color::LightGreen));
        assert!(matches!(parse_color("lightyellow"), Color::LightYellow));
        assert!(matches!(parse_color("lightblue"), Color::LightBlue));
        assert!(matches!(parse_color("lightmagenta"), Color::LightMagenta));
        assert!(matches!(parse_color("lightcyan"), Color::LightCyan));
        assert!(matches!(parse_color("reset"), Color::Reset));
        assert!(matches!(parse_color("terminal"), Color::Reset));
        assert!(matches!(parse_color("default"), Color::Reset));
    }

    #[test]
    fn test_parse_color_invalid_fallback() {
        // Invalid formats should fallback to white
        let color = parse_color("invalid_color");
        assert!(matches!(color, Color::White));

        let color = parse_color("#ZZZ");
        assert!(matches!(color, Color::White));

        let color = parse_color("300, 300, 300"); // Out of range
        assert!(matches!(color, Color::White));
    }

    #[test]
    fn test_custom_theme_deserialization() {
        let toml_str = r##"
[theme]
active = "my-custom"

[theme.custom.my-custom]
active_border = "#7e9cd8"
inactive_border = "darkgray"
searching_border = "255, 160, 102"
selected_bg = "#252b37"
selected_inactive_bg = "#1e222c"
visual_bg = "70, 130, 180"
timer_active_bg = "green"
row_alternate_bg = "#16191f"
edit_bg = "#223d50"
focus_bg = "magenta"
primary_text = "white"
secondary_text = "gray"
highlight_text = "cyan"
success = "lightgreen"
warning = "yellow"
error = "lightred"
info = "cyan"
timer_text = "#ffc777"
badge = "lightmagenta"
        "##;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.theme.active, "my-custom");
        assert!(config.theme.custom.contains_key("my-custom"));

        let custom_colors = config.theme.custom.get("my-custom").unwrap();
        assert_eq!(custom_colors.active_border, "#7e9cd8");
        assert_eq!(custom_colors.timer_active_bg, "green");
        assert_eq!(custom_colors.searching_border, "255, 160, 102");
    }

    #[test]
    fn test_custom_theme_from_config() {
        let mut custom = HashMap::new();
        custom.insert(
            "test-theme".to_string(),
            CustomThemeColors {
                active_border: "#FF0000".to_string(),
                inactive_border: "darkgray".to_string(),
                searching_border: "yellow".to_string(),
                selected_bg: "#252b37".to_string(),
                selected_inactive_bg: "#1e222c".to_string(),
                visual_bg: "blue".to_string(),
                timer_active_bg: "green".to_string(),
                row_alternate_bg: "#16191f".to_string(),
                edit_bg: "cyan".to_string(),
                focus_bg: "magenta".to_string(),
                primary_text: "white".to_string(),
                secondary_text: "gray".to_string(),
                highlight_text: "cyan".to_string(),
                success: "green".to_string(),
                warning: "yellow".to_string(),
                error: "red".to_string(),
                info: "cyan".to_string(),
                timer_text: "yellow".to_string(),
                badge: "magenta".to_string(),
            },
        );

        let theme_config = ThemeConfig {
            active: "test-theme".to_string(),
            custom,
        };

        let theme = theme_config.get_active_theme();
        // Verify custom theme is applied (check the custom red border)
        assert!(matches!(theme.active_border, Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn test_fallback_to_default_when_custom_not_found() {
        let theme_config = ThemeConfig {
            active: "non-existent-theme".to_string(),
            custom: HashMap::new(),
        };

        let theme = theme_config.get_active_theme();
        // Should fallback to default theme
        assert!(matches!(theme.active_border, Color::Cyan));
        assert!(matches!(theme.error, Color::LightRed));
    }

    #[test]
    fn test_theme_from_custom_colors() {
        let custom_colors = CustomThemeColors {
            active_border: "#7e9cd8".to_string(),
            inactive_border: "darkgray".to_string(),
            searching_border: "yellow".to_string(),
            selected_bg: "50, 50, 70".to_string(),
            selected_inactive_bg: "#1e222c".to_string(),
            visual_bg: "blue".to_string(),
            timer_active_bg: "green".to_string(),
            row_alternate_bg: "#000000".to_string(),
            edit_bg: "cyan".to_string(),
            focus_bg: "magenta".to_string(),
            primary_text: "white".to_string(),
            secondary_text: "gray".to_string(),
            highlight_text: "cyan".to_string(),
            success: "green".to_string(),
            warning: "yellow".to_string(),
            error: "red".to_string(),
            info: "cyan".to_string(),
            timer_text: "255, 199, 119".to_string(),
            badge: "magenta".to_string(),
        };

        let theme = Theme::from_custom(&custom_colors);

        // Verify different color formats are parsed correctly
        assert!(matches!(theme.active_border, Color::Rgb(126, 156, 216))); // hex
        assert!(matches!(theme.inactive_border, Color::DarkGray)); // named
        assert!(matches!(theme.selected_bg, Color::Rgb(50, 50, 70))); // rgb tuple
        assert!(matches!(theme.timer_text, Color::Rgb(255, 199, 119))); // rgb tuple
    }

    #[test]
    fn test_config_get_theme() {
        let mut custom = HashMap::new();
        custom.insert(
            "custom1".to_string(),
            CustomThemeColors {
                active_border: "red".to_string(),
                inactive_border: "darkgray".to_string(),
                searching_border: "yellow".to_string(),
                selected_bg: "blue".to_string(),
                selected_inactive_bg: "black".to_string(),
                visual_bg: "cyan".to_string(),
                timer_active_bg: "green".to_string(),
                row_alternate_bg: "black".to_string(),
                edit_bg: "blue".to_string(),
                focus_bg: "magenta".to_string(),
                primary_text: "white".to_string(),
                secondary_text: "gray".to_string(),
                highlight_text: "cyan".to_string(),
                success: "green".to_string(),
                warning: "yellow".to_string(),
                error: "red".to_string(),
                info: "cyan".to_string(),
                timer_text: "yellow".to_string(),
                badge: "magenta".to_string(),
            },
        );

        let config = Config {
            integrations: IntegrationConfig::default(),
            theme: ThemeConfig {
                active: "custom1".to_string(),
                custom,
            },
            columns: ColumnVisibilityConfig::default(),
        };

        let theme = config.get_theme();
        assert!(matches!(theme.active_border, Color::Red));
    }

    // Additional comprehensive tests

    #[test]
    fn test_all_predefined_theme_methods() {
        // Test that all theme methods return valid themes with all fields populated
        let default = Theme::default_theme();
        let kanagawa = Theme::kanagawa();
        let catppuccin = Theme::catppuccin();
        let gruvbox = Theme::gruvbox();
        let monokai = Theme::monokai();
        let dracula = Theme::dracula();
        let everforest = Theme::everforest();
        let terminal = Theme::terminal();

        // Verify each theme has different active_border colors (unique themes)
        assert!(matches!(default.active_border, Color::Cyan));
        assert!(matches!(kanagawa.active_border, Color::Rgb(127, 180, 202)));
        assert!(matches!(
            catppuccin.active_border,
            Color::Rgb(137, 180, 250)
        ));
        assert!(matches!(gruvbox.active_border, Color::Rgb(131, 165, 152)));
        assert!(matches!(monokai.active_border, Color::Rgb(102, 217, 239)));
        assert!(matches!(dracula.active_border, Color::Rgb(139, 233, 253)));
        assert!(matches!(
            everforest.active_border,
            Color::Rgb(131, 192, 146)
        ));
        assert!(matches!(terminal.active_border, Color::Cyan));
    }

    #[test]
    fn test_parse_color_hex_edge_cases() {
        // Test lowercase hex
        assert!(matches!(parse_color("#ffffff"), Color::Rgb(255, 255, 255)));
        assert!(matches!(parse_color("#000000"), Color::Rgb(0, 0, 0)));

        // Test uppercase hex
        assert!(matches!(parse_color("#FFFFFF"), Color::Rgb(255, 255, 255)));
        assert!(matches!(parse_color("#ABC"), Color::Rgb(170, 187, 204)));

        // Test mixed case
        assert!(matches!(parse_color("#FfFfFf"), Color::Rgb(255, 255, 255)));

        // Test invalid hex (wrong length)
        assert!(matches!(parse_color("#FF"), Color::White)); // fallback
        assert!(matches!(parse_color("#FFFFFFF"), Color::White)); // fallback

        // Test invalid hex characters
        assert!(matches!(parse_color("#GGGGGG"), Color::White)); // fallback
        assert!(matches!(parse_color("#XYZ"), Color::White)); // fallback
    }

    #[test]
    fn test_parse_color_rgb_edge_cases() {
        // Test boundary values
        assert!(matches!(parse_color("0, 0, 0"), Color::Rgb(0, 0, 0)));
        assert!(matches!(
            parse_color("255, 255, 255"),
            Color::Rgb(255, 255, 255)
        ));

        // Test with parentheses (should be handled by trim)
        assert!(matches!(
            parse_color("(100, 150, 200)"),
            Color::Rgb(100, 150, 200)
        ));

        // Test various spacing
        assert!(matches!(parse_color("10,20,30"), Color::Rgb(10, 20, 30)));
        assert!(matches!(
            parse_color("  50  ,  100  ,  150  "),
            Color::Rgb(50, 100, 150)
        ));

        // Test invalid RGB values
        assert!(matches!(parse_color("256, 100, 100"), Color::White)); // out of range
        assert!(matches!(parse_color("100, 300, 100"), Color::White)); // out of range
        assert!(matches!(parse_color("100, 100, 256"), Color::White)); // out of range
        assert!(matches!(parse_color("-1, 100, 100"), Color::White)); // negative
        assert!(matches!(parse_color("abc, 100, 100"), Color::White)); // non-numeric

        // Test wrong number of components
        assert!(matches!(parse_color("100, 100"), Color::White)); // too few
        assert!(matches!(parse_color("100, 100, 100, 100"), Color::White)); // too many
    }

    #[test]
    fn test_parse_color_named_variations() {
        // Test case variations
        assert!(matches!(parse_color("RED"), Color::Red));
        assert!(matches!(parse_color("Red"), Color::Red));
        assert!(matches!(parse_color("rEd"), Color::Red));

        // Test grey vs gray
        assert!(matches!(parse_color("gray"), Color::Gray));
        assert!(matches!(parse_color("grey"), Color::Gray));
        assert!(matches!(parse_color("GRAY"), Color::Gray));
        assert!(matches!(parse_color("GREY"), Color::Gray));
        assert!(matches!(parse_color("darkgray"), Color::DarkGray));
        assert!(matches!(parse_color("darkgrey"), Color::DarkGray));

        // Test all light colors
        assert!(matches!(parse_color("LightRed"), Color::LightRed));
        assert!(matches!(parse_color("LightGreen"), Color::LightGreen));
        assert!(matches!(parse_color("LightYellow"), Color::LightYellow));
        assert!(matches!(parse_color("LightBlue"), Color::LightBlue));
        assert!(matches!(parse_color("LightMagenta"), Color::LightMagenta));
        assert!(matches!(parse_color("LightCyan"), Color::LightCyan));

        // Test terminal color aliases
        assert!(matches!(parse_color("reset"), Color::Reset));
        assert!(matches!(parse_color("terminal"), Color::Reset));
        assert!(matches!(parse_color("default"), Color::Reset));
        assert!(matches!(parse_color("RESET"), Color::Reset));
        assert!(matches!(parse_color("Terminal"), Color::Reset));
    }

    #[test]
    fn test_parse_color_whitespace_handling() {
        // Test leading/trailing whitespace
        assert!(matches!(parse_color("  red  "), Color::Red));
        assert!(matches!(parse_color("\tblue\t"), Color::Blue));
        assert!(matches!(parse_color(" #FF0000 "), Color::Rgb(255, 0, 0)));
        assert!(matches!(
            parse_color("  100, 200, 150  "),
            Color::Rgb(100, 200, 150)
        ));
    }

    #[test]
    fn test_parse_color_empty_and_whitespace() {
        // Empty strings should fallback to white
        assert!(matches!(parse_color(""), Color::White));
        assert!(matches!(parse_color("   "), Color::White));
        assert!(matches!(parse_color("\t\t"), Color::White));
    }

    #[test]
    fn test_theme_color_consistency() {
        // Verify that all pre-defined themes have consistent structure
        // (all 18 colors are present and valid)
        let themes = vec![
            Theme::default_theme(),
            Theme::kanagawa(),
            Theme::catppuccin(),
            Theme::gruvbox(),
            Theme::monokai(),
            Theme::dracula(),
            Theme::everforest(),
            Theme::terminal(),
        ];

        for theme in themes {
            // Just access all fields to ensure they exist
            let _ = theme.active_border;
            let _ = theme.inactive_border;
            let _ = theme.searching_border;
            let _ = theme.selected_bg;
            let _ = theme.selected_inactive_bg;
            let _ = theme.visual_bg;
            let _ = theme.timer_active_bg;
            let _ = theme.row_alternate_bg;
            let _ = theme.edit_bg;
            let _ = theme.primary_text;
            let _ = theme.secondary_text;
            let _ = theme.highlight_text;
            let _ = theme.success;
            let _ = theme.warning;
            let _ = theme.error;
            let _ = theme.info;
            let _ = theme.timer_text;
            let _ = theme.badge;
        }
    }

    #[test]
    fn test_custom_theme_colors_all_formats() {
        let custom_colors = CustomThemeColors {
            active_border: "#FF0000".to_string(),        // hex
            inactive_border: "darkgray".to_string(),     // named
            searching_border: "255, 255, 0".to_string(), // rgb
            selected_bg: "#00F".to_string(),             // short hex
            selected_inactive_bg: "Black".to_string(),   // named (capitalized)
            visual_bg: "0, 128, 255".to_string(),        // rgb
            timer_active_bg: "lightgreen".to_string(),   // named
            row_alternate_bg: "#111".to_string(),        // short hex
            edit_bg: "(50, 100, 150)".to_string(),       // rgb with parens
            focus_bg: "magenta".to_string(),             // named
            primary_text: "white".to_string(),           // named
            secondary_text: "128, 128, 128".to_string(), // rgb
            highlight_text: "#0FF".to_string(),          // short hex cyan
            success: "green".to_string(),                // named
            warning: "#FFAA00".to_string(),              // hex
            error: "255, 0, 0".to_string(),              // rgb
            info: "cyan".to_string(),                    // named
            timer_text: "#FFA500".to_string(),           // hex
            badge: "magenta".to_string(),                // named
        };

        let theme = Theme::from_custom(&custom_colors);

        // Verify mixed color formats are parsed correctly
        assert!(matches!(theme.active_border, Color::Rgb(255, 0, 0))); // hex
        assert!(matches!(theme.inactive_border, Color::DarkGray)); // named
        assert!(matches!(theme.searching_border, Color::Rgb(255, 255, 0))); // rgb
        assert!(matches!(theme.selected_bg, Color::Rgb(0, 0, 255))); // short hex
        assert!(matches!(theme.selected_inactive_bg, Color::Black)); // named
        assert!(matches!(theme.visual_bg, Color::Rgb(0, 128, 255))); // rgb
        assert!(matches!(theme.timer_active_bg, Color::LightGreen)); // named
        assert!(matches!(theme.row_alternate_bg, Color::Rgb(17, 17, 17))); // short hex
    }

    #[test]
    fn test_multiple_custom_themes_in_config() {
        let toml_str = r##"
[theme]
active = "theme2"

[theme.custom.theme1]
active_border = "red"
inactive_border = "darkgray"
searching_border = "yellow"
selected_bg = "blue"
selected_inactive_bg = "black"
visual_bg = "cyan"
timer_active_bg = "green"
row_alternate_bg = "black"
edit_bg = "blue"
focus_bg = "magenta"
primary_text = "white"
secondary_text = "gray"
highlight_text = "cyan"
success = "green"
warning = "yellow"
error = "red"
info = "cyan"
timer_text = "yellow"
badge = "magenta"

[theme.custom.theme2]
active_border = "#FF00FF"
inactive_border = "darkgray"
searching_border = "yellow"
selected_bg = "blue"
selected_inactive_bg = "black"
visual_bg = "cyan"
timer_active_bg = "green"
row_alternate_bg = "black"
edit_bg = "blue"
focus_bg = "magenta"
primary_text = "white"
secondary_text = "gray"
highlight_text = "cyan"
success = "green"
warning = "yellow"
error = "red"
info = "cyan"
timer_text = "yellow"
badge = "magenta"
        "##;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.theme.active, "theme2");
        assert_eq!(config.theme.custom.len(), 2);
        assert!(config.theme.custom.contains_key("theme1"));
        assert!(config.theme.custom.contains_key("theme2"));

        // Verify the active theme is theme2 with magenta border
        let theme = config.get_theme();
        assert!(matches!(theme.active_border, Color::Rgb(255, 0, 255)));
    }

    #[test]
    fn test_theme_config_case_sensitivity() {
        // Theme names should be case-sensitive (lowercase by convention)
        let theme_config = ThemeConfig {
            active: "KANAGAWA".to_string(), // uppercase (not found)
            custom: HashMap::new(),
        };

        let theme = theme_config.get_active_theme();
        // Should fallback to default theme (not kanagawa)
        assert!(matches!(theme.active_border, Color::Cyan)); // default theme
    }

    #[test]
    fn test_custom_theme_overrides_predefined() {
        // Custom theme with same name as predefined should override
        let mut custom = HashMap::new();
        custom.insert(
            "default".to_string(),
            CustomThemeColors {
                active_border: "#FF0000".to_string(), // red instead of cyan
                inactive_border: "darkgray".to_string(),
                searching_border: "yellow".to_string(),
                selected_bg: "blue".to_string(),
                selected_inactive_bg: "black".to_string(),
                visual_bg: "cyan".to_string(),
                timer_active_bg: "green".to_string(),
                row_alternate_bg: "black".to_string(),
                edit_bg: "blue".to_string(),
                focus_bg: "magenta".to_string(),
                primary_text: "white".to_string(),
                secondary_text: "gray".to_string(),
                highlight_text: "cyan".to_string(),
                success: "green".to_string(),
                warning: "yellow".to_string(),
                error: "red".to_string(),
                info: "cyan".to_string(),
                timer_text: "yellow".to_string(),
                badge: "magenta".to_string(),
            },
        );

        let theme_config = ThemeConfig {
            active: "default".to_string(),
            custom,
        };

        let theme = theme_config.get_active_theme();
        // Should use custom theme (red), not predefined default (cyan)
        assert!(matches!(theme.active_border, Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_color_rgb_with_parentheses_and_spaces() {
        // RGB tuples can have parentheses (users might include them) - we strip them
        assert!(matches!(
            parse_color("(255, 128, 64)"),
            Color::Rgb(255, 128, 64)
        ));
        assert!(matches!(
            parse_color("( 100 , 200 , 150 )"),
            Color::Rgb(100, 200, 150)
        ));

        // Parentheses are now stripped, so this should parse successfully
        let result = parse_color("(10,20,30)");
        assert!(matches!(result, Color::Rgb(10, 20, 30)));
    }

    #[test]
    fn test_theme_serialization() {
        // Test that ThemeConfig can be serialized/deserialized
        let theme_config = ThemeConfig {
            active: "gruvbox".to_string(),
            custom: HashMap::new(),
        };

        let serialized = toml::to_string(&theme_config).expect("Failed to serialize");
        assert!(serialized.contains("active"));
        assert!(serialized.contains("gruvbox"));

        let deserialized: ThemeConfig = toml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.active, "gruvbox");
        assert!(deserialized.custom.is_empty());
    }
}
