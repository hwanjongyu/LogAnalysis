use gpui::{rgb, Rgba};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg_dark: Rgba,
    pub bg_header: Rgba,
    pub bg_panel: Rgba,
    pub bg_gutter: Rgba,
    pub bg_card: Rgba,
    pub bg_badge: Rgba,
    pub row_hover: Rgba,
    pub border: Rgba,
    pub border_subtle: Rgba,
    pub text_primary: Rgba,
    pub text_muted: Rgba,
    pub text_gutter: Rgba,
    pub selection_bg: Rgba,
    pub highlight_search: Rgba,
    pub btn_hover: Rgba,
    pub accent: Rgba,
    pub accent_muted: Rgba,
    pub error_red: Rgba,
    pub success_green: Rgba,
    pub warning_amber: Rgba,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    pub fn dark() -> Self {
        // Modern Zed / TokyoNight / Catppuccin Mocha inspired Dark Theme
        Self {
            bg_dark: rgb(0x111116),        // Deep obsidian canvas
            bg_header: rgb(0x16161d),      // Sleek top toolbar & status bar
            bg_panel: rgb(0x181820),       // Sidebar background
            bg_gutter: rgb(0x14141b),      // Pinned gutter background
            bg_card: rgb(0x1f1f29),        // Filter rule card surface
            bg_badge: rgb(0x282836),       // Pill badge surface
            row_hover: rgb(0x1c1e28),      // Subtle log line hover
            border: rgb(0x292936),         // Structural divider border
            border_subtle: rgb(0x20202b),  // Subtle card border
            text_primary: rgb(0xe4e4ef),   // High-contrast crisp white-gray
            text_muted: rgb(0x84879a),     // Secondary labels & hints
            text_gutter: rgb(0x525568),    // Muted line numbers
            selection_bg: rgb(0x23304c),   // Rich navy row selection
            highlight_search: rgb(0x4a3818), // Warm amber search highlight
            btn_hover: rgb(0x262733),      // Interactive button surface
            accent: rgb(0x3b82f6),         // Electric blue accent
            accent_muted: rgb(0x1e3a8a),   // Muted active pill background
            error_red: rgb(0xf87171),      // Coral red for Exclude / Stop
            success_green: rgb(0x34d399),  // Emerald green for Live / Active
            warning_amber: rgb(0xfbbf24),  // Amber for Paused / Warning
        }
    }

    pub fn light() -> Self {
        // Clean Daylight Studio Theme
        Self {
            bg_dark: rgb(0xffffff),
            bg_header: rgb(0xf3f4f6),
            bg_panel: rgb(0xf8fafc),
            bg_gutter: rgb(0xf1f5f9),
            bg_card: rgb(0xffffff),
            bg_badge: rgb(0xe2e8f0),
            row_hover: rgb(0xf1f5f9),
            border: rgb(0xe2e8f0),
            border_subtle: rgb(0xedF2f7),
            text_primary: rgb(0x0f172a),
            text_muted: rgb(0x64748b),
            text_gutter: rgb(0x94a3b8),
            selection_bg: rgb(0xdbeafe),
            highlight_search: rgb(0xfef08a),
            btn_hover: rgb(0xe2e8f0),
            accent: rgb(0x2563eb),
            accent_muted: rgb(0xbfdbfe),
            error_red: rgb(0xdc2626),
            success_green: rgb(0x16a34a),
            warning_amber: rgb(0xd97706),
        }
    }
}
