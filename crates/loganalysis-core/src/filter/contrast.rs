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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srgb_to_linear_low_and_high_branches() {
        // Low linear branch: c_byte / 255.0 <= 0.03928 holds for 0..=10
        assert_eq!(srgb_to_linear(0), 0.0);
        let expected_10 = (10.0 / 255.0) / 12.92;
        assert!((srgb_to_linear(10) - expected_10).abs() < 1e-12);

        // High gamma branch: c_byte / 255.0 > 0.03928 holds for 11..=255
        let expected_11 = ((11.0_f64 / 255.0 + 0.055) / 1.055).powf(2.4);
        assert!((srgb_to_linear(11) - expected_11).abs() < 1e-12);
        assert!((srgb_to_linear(255) - 1.0).abs() < 1e-12);

        assert!(srgb_to_linear(0) < srgb_to_linear(10));
        assert!(srgb_to_linear(10) < srgb_to_linear(11));
        assert!(srgb_to_linear(11) < srgb_to_linear(255));
    }

    #[test]
    fn test_relative_luminance_extremes_and_channel_weights() {
        assert_eq!(relative_luminance(0x000000), 0.0);
        assert!((relative_luminance(0xFFFFFF) - 1.0).abs() < 1e-12);

        let l_green = relative_luminance(0x00FF00);
        let l_red = relative_luminance(0xFF0000);
        let l_blue = relative_luminance(0x0000FF);
        assert!(l_green > l_red);
        assert!(l_red > l_blue);
    }

    #[test]
    fn test_contrast_ratio_bounds_and_symmetry() {
        assert_eq!(contrast_ratio(0x3A7BD5, 0x3A7BD5), 1.0);
        assert!((contrast_ratio(0x000000, 0xFFFFFF) - 21.0).abs() < 1e-12);

        let ab = contrast_ratio(0xFFB6B6, 0x8B0000);
        let ba = contrast_ratio(0x8B0000, 0xFFB6B6);
        assert!((ab - ba).abs() < 1e-12);
    }

    #[test]
    fn test_evaluate_contrast_levels_and_boundaries() {
        // High contrast Pass (>= 4.5)
        let (bw_ratio, bw_level) = evaluate_contrast(0x000000, 0xFFFFFF);
        assert!((bw_ratio - 21.0).abs() < 1e-12);
        assert_eq!(bw_level, ContrastLevel::Pass);

        // Low contrast Critical (< 3.0)
        let (low_ratio, low_level) = evaluate_contrast(0x808080, 0x999999);
        assert!(low_ratio < 3.0);
        assert_eq!(low_level, ContrastLevel::Critical);

        // Exact threshold boundary around 3.0 (against black 0x000000)
        let (below_3, below_3_level) = evaluate_contrast(0x000000, 0x595959);
        assert!(below_3 < 3.0, "expected < 3.0, got {below_3}");
        assert_eq!(below_3_level, ContrastLevel::Critical);

        let (above_3, above_3_level) = evaluate_contrast(0x000000, 0x5A5A5A);
        assert!(
            (3.0..4.5).contains(&above_3),
            "expected in [3.0, 4.5), got {above_3}"
        );
        assert_eq!(above_3_level, ContrastLevel::Warning);

        // Exact threshold boundary around 4.5 (against black 0x000000)
        let (below_45, below_45_level) = evaluate_contrast(0x000000, 0x747474);
        assert!(
            (3.0..4.5).contains(&below_45),
            "expected in [3.0, 4.5), got {below_45}"
        );
        assert_eq!(below_45_level, ContrastLevel::Warning);

        let (above_45, above_45_level) = evaluate_contrast(0x000000, 0x757575);
        assert!(above_45 >= 4.5, "expected >= 4.5, got {above_45}");
        assert_eq!(above_45_level, ContrastLevel::Pass);
    }
}
