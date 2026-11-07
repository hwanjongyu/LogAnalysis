use gpui::{rgb, rgba, Rgba};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg_canvas: Rgba,
    pub bg_dark: Rgba,
    pub bg_header: Rgba,
    pub bg_panel: Rgba,
    pub bg_gutter: Rgba,
    pub bg_card: Rgba,
    pub bg_viewport: Rgba,
    pub bg_badge: Rgba,
    pub row_hover: Rgba,
    pub border: Rgba,
    pub border_subtle: Rgba,
    pub border_highlight: Rgba,
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
    pub kbd_bg: Rgba,
    pub kbd_fg: Rgba,
    pub kbd_border: Rgba,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

// 8-color curated high-contrast pastel palettes for deterministic tag color hashing
pub const DARK_TAG_HEX: [u32; 8] = [
    0x2dd4bf, // Teal 400
    0x38bdf8, // Sky 400
    0x818cf8, // Indigo 400
    0xa78bfa, // Violet 400
    0xfb7185, // Rose 400
    0x34d399, // Emerald 400
    0xfbbf24, // Amber 400
    0x22d3ee, // Cyan 400
];

pub const LIGHT_TAG_HEX: [u32; 8] = [
    0x0f766e, // Teal 700
    0x0369a1, // Sky 700
    0x4338ca, // Indigo 700
    0x6d28d9, // Violet 700
    0xbe123c, // Rose 700
    0x047857, // Emerald 700
    0xb45309, // Amber 700
    0x0e7490, // Cyan 700
];

/// Computes a deterministic tag color using FNV-1a hashing against curated palettes
pub fn hash_tag_color(tag: &str, is_dark: bool) -> Rgba {
    let clean = tag.trim();
    if clean.is_empty() || clean == "-" {
        return if is_dark { rgb(0x9ca3af) } else { rgb(0x64748b) };
    }
    let mut hash: u32 = 2166136261;
    for b in clean.as_bytes() {
        hash = (hash ^ (*b as u32)).wrapping_mul(16777619);
    }
    let idx = (hash % 8) as usize;
    if is_dark {
        rgb(DARK_TAG_HEX[idx])
    } else {
        rgb(LIGHT_TAG_HEX[idx])
    }
}

/// Provides WCAG-compliant background and foreground colors for severity level micro-badges
pub fn severity_badge_colors(level_char: Option<char>, is_dark: bool) -> (Rgba, Rgba) {
    if is_dark {
        match level_char {
            Some('E') | Some('F') => (rgb(0x450a0a), rgb(0xfca5a5)), // Dark red surface + crisp pastel red
            Some('W') => (rgb(0x451a03), rgb(0xfde68a)),             // Dark amber surface + pastel amber
            Some('I') => (rgb(0x064e3b), rgb(0x6ee7b7)),             // Dark emerald surface + pastel emerald
            Some('D') => (rgb(0x164e63), rgb(0x67e8f9)),             // Dark cyan surface + pastel cyan
            Some('V') => (rgb(0x1e293b), rgb(0x94a3b8)),             // Dark slate surface + muted slate
            _ => (rgb(0x1f202b), rgb(0x84879a)),
        }
    } else {
        match level_char {
            Some('E') | Some('F') => (rgb(0xfee2e2), rgb(0xb91c1c)), // Soft red surface + rich red text
            Some('W') => (rgb(0xfef3c7), rgb(0xb45309)),             // Soft amber surface + rich amber text
            Some('I') => (rgb(0xd1fae5), rgb(0x047857)),             // Soft emerald surface + rich emerald text
            Some('D') => (rgb(0xcffafe), rgb(0x0e7490)),             // Soft cyan surface + rich cyan text
            Some('V') => (rgb(0xf1f5f9), rgb(0x475569)),             // Soft slate surface + slate text
            _ => (rgb(0xe2e8f0), rgb(0x64748b)),
        }
    }
}

impl Theme {
    pub fn dark() -> Self {
        // Modern Zed / Linear / TokyoNight inspired 4-Tier Elevation Dark Theme
        Self {
            bg_canvas: rgb(0x0e0f14),       // Layer 0: Obsidian canvas
            bg_dark: rgb(0x12131a),         // Base background
            bg_header: rgb(0x14151d),       // Layer 1: Sleek top toolbar & window titlebar
            bg_panel: rgb(0x161722),        // Layer 1: Sidebar & docked panels
            bg_gutter: rgb(0x121319),       // Pinned gutter background
            bg_viewport: rgb(0x13141c),     // Layer 2: Main log viewport canvas
            bg_card: rgb(0x1c1e2a),         // Layer 3: Filter cards & floating popovers
            bg_badge: rgb(0x232636),        // Layer 4: Micro pills & kbd badges
            row_hover: rgb(0x1c1e29),       // Subtle log line hover
            border: rgba(0xffffff18),       // 1px structural divider border (translucent white)
            border_subtle: rgba(0xffffff0d),// 1px subtle inner border
            border_highlight: rgba(0xffffff26), // 1px glassy top highlight reflection
            text_primary: rgb(0xe6e7ee),    // High-contrast crisp white-gray
            text_muted: rgb(0x8b8e9f),      // Secondary labels & telemetry hints
            text_gutter: rgb(0x56596e),     // Muted line numbers
            selection_bg: rgb(0x233354),    // Rich navy row selection
            highlight_search: rgb(0x4a3818),// Warm amber search highlight
            btn_hover: rgb(0x252737),       // Interactive button hover surface
            accent: rgb(0x3b82f6),          // Electric blue accent
            accent_muted: rgb(0x1e3a8a),    // Muted active pill background
            error_red: rgb(0xf87171),       // Coral red for Exclude / Errors
            success_green: rgb(0x34d399),   // Emerald green for Live / Active
            warning_amber: rgb(0xfbbf24),   // Amber for Paused / Warnings
            kbd_bg: rgb(0x202230),          // Shortcut badge background
            kbd_fg: rgb(0x9ca3af),          // Shortcut badge text
            kbd_border: rgba(0xffffff18),   // Shortcut badge border
        }
    }

    pub fn light() -> Self {
        // Clean Daylight Studio Theme
        Self {
            bg_canvas: rgb(0xf8fafc),
            bg_dark: rgb(0xffffff),
            bg_header: rgb(0xf1f5f9),
            bg_panel: rgb(0xf8fafc),
            bg_gutter: rgb(0xf1f5f9),
            bg_viewport: rgb(0xffffff),
            bg_card: rgb(0xffffff),
            bg_badge: rgb(0xe2e8f0),
            row_hover: rgb(0xf1f5f9),
            border: rgba(0x00000014),
            border_subtle: rgba(0x0000000a),
            border_highlight: rgba(0xffffffcc),
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
            kbd_bg: rgb(0xf1f5f9),
            kbd_fg: rgb(0x475569),
            kbd_border: rgb(0xcbd5e1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_tag_color_distribution() {
        let sample_tags = [
            "ActivityManager",
            "WindowManager",
            "SurfaceFlinger",
            "InputDispatcher",
            "AudioFlinger",
            "PackageManager",
            "CameraService",
            "ConnectivityService",
        ];

        let mut dark_colors = Vec::new();
        let mut light_colors = Vec::new();

        for tag in sample_tags {
            let dark_c = hash_tag_color(tag, true);
            let light_c = hash_tag_color(tag, false);
            dark_colors.push(dark_c);
            light_colors.push(light_c);
        }

        // Ensure deterministic: same tag always produces exact same color
        assert_eq!(
            hash_tag_color("ActivityManager", true),
            dark_colors[0]
        );
        assert_eq!(
            hash_tag_color("ActivityManager", false),
            light_colors[0]
        );

        // Ensure palette has entropy across multiple tags
        let unique_dark: std::collections::HashSet<_> = dark_colors
            .iter()
            .map(|c| (c.r.to_bits(), c.g.to_bits(), c.b.to_bits()))
            .collect();
        assert!(
            unique_dark.len() >= 4,
            "Tag color hashing should distribute across palette"
        );
    }

    #[test]
    fn test_severity_badge_colors_coverage() {
        let levels = [Some('E'), Some('F'), Some('W'), Some('I'), Some('D'), Some('V'), None];
        for &lvl in &levels {
            let (dark_bg, dark_fg) = severity_badge_colors(lvl, true);
            let (light_bg, light_fg) = severity_badge_colors(lvl, false);

            // Verify alpha is opaque or valid
            assert!(dark_bg.a > 0.0);
            assert!(dark_fg.a > 0.0);
            assert!(light_bg.a > 0.0);
            assert!(light_fg.a > 0.0);

            // Background and foreground must be distinct for readability
            assert_ne!(
                (dark_bg.r, dark_bg.g, dark_bg.b),
                (dark_fg.r, dark_fg.g, dark_fg.b)
            );
            assert_ne!(
                (light_bg.r, light_bg.g, light_bg.b),
                (light_fg.r, light_fg.g, light_fg.b)
            );
        }
    }

    #[test]
    fn test_theme_surface_tokens() {
        let dark = Theme::dark();
        assert_ne!(dark.bg_canvas.r, dark.bg_card.r);
        assert_ne!(dark.bg_panel.r, dark.bg_badge.r);

        let light = Theme::light();
        assert_ne!(light.bg_panel.r, light.accent.r);
        assert!(light.kbd_fg.a > 0.0);
    }
}
