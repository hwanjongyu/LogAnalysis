use gpui::{rgb, Rgba};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg_dark: Rgba,
    pub bg_header: Rgba,
    pub bg_panel: Rgba,
    pub bg_gutter: Rgba,
    pub border: Rgba,
    pub text_primary: Rgba,
    pub text_muted: Rgba,
    pub text_gutter: Rgba,
    pub selection_bg: Rgba,
    pub highlight_search: Rgba,
    pub btn_hover: Rgba,
    pub accent: Rgba,
    pub error_red: Rgba,
    pub success_green: Rgba,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_dark: rgb(0x1a1a1e),
            bg_header: rgb(0x121214),
            bg_panel: rgb(0x1e1e24),
            bg_gutter: rgb(0x222228),
            border: rgb(0x2e2e34),
            text_primary: rgb(0xe2e2e6),
            text_muted: rgb(0x8a8a90),
            text_gutter: rgb(0x606068),
            selection_bg: rgb(0x2d3748),
            highlight_search: rgb(0x3e2723),
            btn_hover: rgb(0x2a2a32),
            accent: rgb(0x3b82f6),
            error_red: rgb(0xef4444),
            success_green: rgb(0x22c55e),
        }
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self::default()
    }

    pub fn light() -> Self {
        Self {
            bg_dark: rgb(0xffffff),
            bg_header: rgb(0xf3f4f6),
            bg_panel: rgb(0xf9fafb),
            bg_gutter: rgb(0xe5e7eb),
            border: rgb(0xd1d5db),
            text_primary: rgb(0x111827),
            text_muted: rgb(0x6b7280),
            text_gutter: rgb(0x9ca3af),
            selection_bg: rgb(0xdbeafe),
            highlight_search: rgb(0xfef08a),
            btn_hover: rgb(0xe5e7eb),
            accent: rgb(0x2563eb),
            error_red: rgb(0xdc2626),
            success_green: rgb(0x16a34a),
        }
    }
}
