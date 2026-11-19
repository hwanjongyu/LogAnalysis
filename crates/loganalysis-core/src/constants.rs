//! Shared constants, color maps, and buffer limits for LogAnalysis.

pub const MAX_MONITOR_LINES: usize = 200_000;
pub const MONITOR_TRIM_BATCH: usize = 20_000;
pub const ASYNC_SEARCH_THRESHOLD: usize = 50_000;

/// Background color map matching original LogAnalysis GUI
pub const COLOR_MAP: &[(&str, &str, u32)] = &[
    ("Khaki", "#F0E68C", 0xF0E68C),
    ("Yellow", "#FFFF00", 0xFFFF00),
    ("Gold", "#FFD700", 0xFFD700),
    ("Cyan", "#00FFFF", 0x00FFFF),
    ("Aqua", "#00FFFF", 0x00FFFF),
    ("Green", "#90EE90", 0x90EE90),
    ("Lime", "#00FF00", 0x00FF00),
    ("PaleGreen", "#98FB98", 0x98FB98),
    ("Red", "#FFB6B6", 0xFFB6B6),
    ("Salmon", "#FA8072", 0xFA8072),
    ("Coral", "#FF7F50", 0xFF7F50),
    ("Blue", "#B6D0FF", 0xB6D0FF),
    ("SkyBlue", "#87CEEB", 0x87CEEB),
    ("LightBlue", "#ADD8E6", 0xADD8E6),
    ("Gray", "#D3D3D3", 0xD3D3D3),
    ("Silver", "#C0C0C0", 0xC0C0C0),
    ("White", "#FFFFFF", 0xFFFFFF),
    ("Orange", "#FFD580", 0xFFD580),
    ("Wheat", "#F5DEB3", 0xF5DEB3),
    ("Purple", "#E6E6FA", 0xE6E6FA),
    ("Plum", "#DDA0DD", 0xDDA0DD),
    ("Orchid", "#DA70D6", 0xDA70D6),
    ("Brown", "#EEDFCC", 0xEEDFCC),
    ("Pink", "#FFD1DC", 0xFFD1DC),
    ("HotPink", "#FF69B4", 0xFF69B4),
    ("Violet", "#F3E5F5", 0xF3E5F5),
    ("Navy", "#B0C4DE", 0xB0C4DE),
    ("Teal", "#B2DFDB", 0xB2DFDB),
    ("Olive", "#F5F5DC", 0xF5F5DC),
    ("Maroon", "#F4CCCC", 0xF4CCCC),
    // Dark-surface background colors
    ("DarkRed", "#450A0A", 0x450A0A),
    ("DarkRose", "#4C0519", 0x4C0519),
    ("DarkOrange", "#451A03", 0x451A03),
    ("DarkAmber", "#422006", 0x422006),
    ("DarkGreen", "#064E3B", 0x064E3B),
    ("DarkTeal", "#134E4A", 0x134E4A),
    ("DarkCyan", "#164E63", 0x164E63),
    ("DarkBlue", "#1E3A5F", 0x1E3A5F),
    ("DarkIndigo", "#312E81", 0x312E81),
    ("DarkPurple", "#3B0764", 0x3B0764),
    ("DarkSlate", "#1E293B", 0x1E293B),
];

/// Text foreground color map matching original LogAnalysis GUI
pub const TEXT_COLOR_MAP: &[(&str, &str, u32)] = &[
    ("Black", "#000000", 0x000000),
    ("Red", "#FF0000", 0xFF0000),
    ("DarkRed", "#8B0000", 0x8B0000),
    ("Crimson", "#DC143C", 0xDC143C),
    ("Blue", "#0000FF", 0x0000FF),
    ("DarkBlue", "#00008B", 0x00008B),
    ("RoyalBlue", "#4169E1", 0x4169E1),
    ("Green", "#008000", 0x008000),
    ("DarkGreen", "#006400", 0x006400),
    ("SeaGreen", "#2E8B57", 0x2E8B57),
    ("Gray", "#808080", 0x808080),
    ("DarkGray", "#A9A9A9", 0xA9A9A9),
    ("White", "#FFFFFF", 0xFFFFFF),
    ("Orange", "#FFA500", 0xFFA500),
    ("DarkOrange", "#FF8C00", 0xFF8C00),
    ("Purple", "#800080", 0x800080),
    ("DarkMagenta", "#8B008B", 0x8B008B),
    ("Indigo", "#4B0082", 0x4B0082),
    ("Brown", "#A52A2A", 0xA52A2A),
    ("SaddleBrown", "#8B4513", 0x8B4513),
    ("Pink", "#FFC0CB", 0xFFC0CB),
    ("DeepPink", "#FF1493", 0xFF1493),
    ("Violet", "#EE82EE", 0xEE82EE),
    ("Navy", "#000080", 0x000080),
    ("Teal", "#008080", 0x008080),
    ("Olive", "#808000", 0x808000),
    ("Maroon", "#800000", 0x800000),
    // Light/dark WCAG-balanced text colors
    ("DeepAmber", "#B45309", 0xB45309),
    ("Slate", "#475569", 0x475569),
    ("CoralRed", "#FCA5A5", 0xFCA5A5),
    ("Rose", "#FB7185", 0xFB7185),
    ("Amber", "#FBBF24", 0xFBBF24),
    ("WarmAmber", "#FDE68A", 0xFDE68A),
    ("Mint", "#6EE7B7", 0x6EE7B7),
    ("LightTeal", "#2DD4BF", 0x2DD4BF),
    ("BrightCyan", "#22D3EE", 0x22D3EE),
    ("Sky", "#38BDF8", 0x38BDF8),
    ("Periwinkle", "#A5B4FC", 0xA5B4FC),
    ("Lavender", "#A78BFA", 0xA78BFA),
    ("LightSlate", "#CBD5E1", 0xCBD5E1),
];

/// Parses a color string (either color name or `#RRGGBB` hex string) into a 24-bit RGB integer `0xRRGGBB`.
pub fn parse_color_spec(spec: &str, is_bg: bool) -> Option<u32> {
    let trimmed = spec.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }

    let map = if is_bg { COLOR_MAP } else { TEXT_COLOR_MAP };
    for &(name, _hex_str, val) in map {
        if name.eq_ignore_ascii_case(trimmed) {
            return Some(val);
        }
    }

    // Try hex parsing "#RRGGBB" or "RRGGBB"
    let hex_part = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex_part.len() == 6 && hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        u32::from_str_radix(hex_part, 16).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_spec_rejects_signed_or_invalid_hex() {
        assert_eq!(parse_color_spec("", true), None);
        assert_eq!(parse_color_spec("   ", false), None);
        assert_eq!(parse_color_spec("None", true), None);
        assert_eq!(parse_color_spec("none", false), None);
        assert_eq!(parse_color_spec("+12345", true), None);
        assert_eq!(parse_color_spec("#+12345", false), None);
        assert_eq!(parse_color_spec("-12345", true), None);
        assert_eq!(parse_color_spec("#12345G", true), None);
        assert_eq!(parse_color_spec("12345G", false), None);
        assert_eq!(parse_color_spec("#123", true), None);
        assert_eq!(parse_color_spec("#1234567", false), None);
        assert_eq!(parse_color_spec("#1A2B3C", true), Some(0x1A2B3C));
        assert_eq!(parse_color_spec("1a2b3c", true), Some(0x1A2B3C));
        assert_eq!(parse_color_spec("  #ff00aa  ", false), Some(0xFF00AA));
    }

    #[test]
    fn test_color_map_and_text_color_map_uniqueness_and_legacy_preservation() {
        use std::collections::HashSet;

        assert_eq!(COLOR_MAP.len(), 41);
        assert_eq!(TEXT_COLOR_MAP.len(), 40);

        let mut bg_names = HashSet::new();
        for &(name, hex_str, val) in COLOR_MAP {
            assert!(
                bg_names.insert(name.to_ascii_lowercase()),
                "Duplicate COLOR_MAP name: {name}"
            );
            assert_eq!(hex_str, format!("#{val:06X}"));
            assert_eq!(parse_color_spec(name, true), Some(val));
            assert_eq!(
                parse_color_spec(&name.to_ascii_lowercase(), true),
                Some(val)
            );
        }

        let mut fg_names = HashSet::new();
        for &(name, hex_str, val) in TEXT_COLOR_MAP {
            assert!(
                fg_names.insert(name.to_ascii_lowercase()),
                "Duplicate TEXT_COLOR_MAP name: {name}"
            );
            assert_eq!(hex_str, format!("#{val:06X}"));
            assert_eq!(parse_color_spec(name, false), Some(val));
            assert_eq!(
                parse_color_spec(&name.to_ascii_lowercase(), false),
                Some(val)
            );
        }

        // Preserve all 30 original COLOR_MAP entries verbatim
        let legacy_bg = [
            ("Khaki", 0xF0E68C),
            ("Yellow", 0xFFFF00),
            ("Gold", 0xFFD700),
            ("Cyan", 0x00FFFF),
            ("Aqua", 0x00FFFF),
            ("Green", 0x90EE90),
            ("Lime", 0x00FF00),
            ("PaleGreen", 0x98FB98),
            ("Red", 0xFFB6B6),
            ("Salmon", 0xFA8072),
            ("Coral", 0xFF7F50),
            ("Blue", 0xB6D0FF),
            ("SkyBlue", 0x87CEEB),
            ("LightBlue", 0xADD8E6),
            ("Gray", 0xD3D3D3),
            ("Silver", 0xC0C0C0),
            ("White", 0xFFFFFF),
            ("Orange", 0xFFD580),
            ("Wheat", 0xF5DEB3),
            ("Purple", 0xE6E6FA),
            ("Plum", 0xDDA0DD),
            ("Orchid", 0xDA70D6),
            ("Brown", 0xEEDFCC),
            ("Pink", 0xFFD1DC),
            ("HotPink", 0xFF69B4),
            ("Violet", 0xF3E5F5),
            ("Navy", 0xB0C4DE),
            ("Teal", 0xB2DFDB),
            ("Olive", 0xF5F5DC),
            ("Maroon", 0xF4CCCC),
        ];
        for (idx, &(name, expected_rgb)) in legacy_bg.iter().enumerate() {
            assert_eq!(COLOR_MAP[idx].0, name);
            assert_eq!(COLOR_MAP[idx].2, expected_rgb);
            assert_eq!(parse_color_spec(name, true), Some(expected_rgb));
        }

        // Verify all 11 new dark-surface COLOR_MAP entries
        let new_bg = [
            ("DarkRed", 0x450A0A),
            ("DarkRose", 0x4C0519),
            ("DarkOrange", 0x451A03),
            ("DarkAmber", 0x422006),
            ("DarkGreen", 0x064E3B),
            ("DarkTeal", 0x134E4A),
            ("DarkCyan", 0x164E63),
            ("DarkBlue", 0x1E3A5F),
            ("DarkIndigo", 0x312E81),
            ("DarkPurple", 0x3B0764),
            ("DarkSlate", 0x1E293B),
        ];
        assert_eq!(COLOR_MAP.len(), legacy_bg.len() + new_bg.len());
        for (offset, &(name, expected_rgb)) in new_bg.iter().enumerate() {
            let idx = legacy_bg.len() + offset;
            assert_eq!(COLOR_MAP[idx].0, name);
            assert_eq!(COLOR_MAP[idx].2, expected_rgb);
            assert_eq!(parse_color_spec(name, true), Some(expected_rgb));
        }

        // Preserve all 27 original TEXT_COLOR_MAP entries verbatim
        let legacy_fg = [
            ("Black", 0x000000),
            ("Red", 0xFF0000),
            ("DarkRed", 0x8B0000),
            ("Crimson", 0xDC143C),
            ("Blue", 0x0000FF),
            ("DarkBlue", 0x00008B),
            ("RoyalBlue", 0x4169E1),
            ("Green", 0x008000),
            ("DarkGreen", 0x006400),
            ("SeaGreen", 0x2E8B57),
            ("Gray", 0x808080),
            ("DarkGray", 0xA9A9A9),
            ("White", 0xFFFFFF),
            ("Orange", 0xFFA500),
            ("DarkOrange", 0xFF8C00),
            ("Purple", 0x800080),
            ("DarkMagenta", 0x8B008B),
            ("Indigo", 0x4B0082),
            ("Brown", 0xA52A2A),
            ("SaddleBrown", 0x8B4513),
            ("Pink", 0xFFC0CB),
            ("DeepPink", 0xFF1493),
            ("Violet", 0xEE82EE),
            ("Navy", 0x000080),
            ("Teal", 0x008080),
            ("Olive", 0x808000),
            ("Maroon", 0x800000),
        ];
        for (idx, &(name, expected_rgb)) in legacy_fg.iter().enumerate() {
            assert_eq!(TEXT_COLOR_MAP[idx].0, name);
            assert_eq!(TEXT_COLOR_MAP[idx].2, expected_rgb);
            assert_eq!(parse_color_spec(name, false), Some(expected_rgb));
        }

        // Verify all 13 new light/dark-optimized TEXT_COLOR_MAP entries
        let new_fg = [
            ("DeepAmber", 0xB45309),
            ("Slate", 0x475569),
            ("CoralRed", 0xFCA5A5),
            ("Rose", 0xFB7185),
            ("Amber", 0xFBBF24),
            ("WarmAmber", 0xFDE68A),
            ("Mint", 0x6EE7B7),
            ("LightTeal", 0x2DD4BF),
            ("BrightCyan", 0x22D3EE),
            ("Sky", 0x38BDF8),
            ("Periwinkle", 0xA5B4FC),
            ("Lavender", 0xA78BFA),
            ("LightSlate", 0xCBD5E1),
        ];
        assert_eq!(TEXT_COLOR_MAP.len(), legacy_fg.len() + new_fg.len());
        for (offset, &(name, expected_rgb)) in new_fg.iter().enumerate() {
            let idx = legacy_fg.len() + offset;
            assert_eq!(TEXT_COLOR_MAP[idx].0, name);
            assert_eq!(TEXT_COLOR_MAP[idx].2, expected_rgb);
            assert_eq!(parse_color_spec(name, false), Some(expected_rgb));
        }

        // Invariant: "Magenta" must NEVER be added to COLOR_MAP
        assert!(!COLOR_MAP
            .iter()
            .any(|&(name, _, _)| name.eq_ignore_ascii_case("Magenta")));
        assert_eq!(parse_color_spec("Magenta", true), None);
        assert_eq!(parse_color_spec("Default", false), None);
    }
}
