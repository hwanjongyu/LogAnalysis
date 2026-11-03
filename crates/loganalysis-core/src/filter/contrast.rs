//! WCAG 2.1 Relative Luminance and Contrast Ratio calculations.

/// Computes linear sRGB component from an 8-bit channel value in [0, 255].
#[inline]
pub fn srgb_to_linear(c_byte: u8) -> f64 {
    let c = c_byte as f64 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Computes relative luminance L in [0, 1] for a 24-bit RGB value `0xRRGGBB`.
pub fn relative_luminance(rgb_hex: u32) -> f64 {
    let r = ((rgb_hex >> 16) & 0xFF) as u8;
    let g = ((rgb_hex >> 8) & 0xFF) as u8;
    let b = (rgb_hex & 0xFF) as u8;

    let r_lin = srgb_to_linear(r);
    let g_lin = srgb_to_linear(g);
    let b_lin = srgb_to_linear(b);

    0.2126 * r_lin + 0.7152 * g_lin + 0.0722 * b_lin
}

/// Computes the WCAG contrast ratio CR in [1, 21] between two 24-bit RGB values.
pub fn contrast_ratio(bg_hex: u32, fg_hex: u32) -> f64 {
    let l_bg = relative_luminance(bg_hex);
    let l_fg = relative_luminance(fg_hex);

    let brightest = l_bg.max(l_fg);
    let darkest = l_bg.min(l_fg);

    (brightest + 0.05) / (darkest + 0.05)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastLevel {
    Pass,
    Warning, // CR in [3.0, 4.5)
    Critical, // CR < 3.0
}

pub fn evaluate_contrast(bg_hex: u32, fg_hex: u32) -> (f64, ContrastLevel) {
    let ratio = contrast_ratio(bg_hex, fg_hex);
    let level = if ratio < 3.0 {
        ContrastLevel::Critical
    } else if ratio < 4.5 {
        ContrastLevel::Warning
    } else {
        ContrastLevel::Pass
    };
    (ratio, level)
}
