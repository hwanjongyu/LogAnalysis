//! Shared constants, color maps, and buffer limits for LogAnalysis.

pub const MAX_MONITOR_LINES: usize = 200_000;
pub const MONITOR_TRIM_BATCH: usize = 20_000;
pub const ASYNC_SEARCH_THRESHOLD: usize = 5_000;

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
    if hex_part.len() == 6 {
        u32::from_str_radix(hex_part, 16).ok()
    } else {
        None
    }
}
