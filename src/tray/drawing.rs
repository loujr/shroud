//! Icon drawing primitives — pure pixel-level functions, easily testable.
//!
//! These are the drawing helpers from `icons.rs` exposed for testing.
//! Each function takes pixel coordinates and returns an ARGB color.

/// Draw three horizontal dots (connecting animation).
pub fn draw_dots(x: i32, y: i32, center: i32, size: i32, fg: &[u8; 4], bg: &[u8; 4]) -> [u8; 4] {
    let rx = x - center;
    let ry = y - center;
    let s = size as f32 / 32.0;
    let dot_r = (2.5 * s) as i32;
    let dot_r_sq = dot_r * dot_r;

    for dot_offset in [-6, 0, 6] {
        let dot_x = (dot_offset as f32 * s) as i32;
        let dx = rx - dot_x;
        if dx * dx + ry * ry <= dot_r_sq {
            return *fg;
        }
    }
    *bg
}

/// Draw a horizontal dash (disconnected indicator).
pub fn draw_dash(x: i32, y: i32, center: i32, size: i32, fg: &[u8; 4], bg: &[u8; 4]) -> [u8; 4] {
    let rx = x - center;
    let ry = y - center;
    let s = size as f32 / 32.0;

    let half_w = (8.0 * s) as i32;
    let half_h = (2.5 * s) as i32;

    if rx.abs() <= half_w && ry.abs() <= half_h {
        *fg
    } else {
        *bg
    }
}

/// Draw an X mark (failed indicator).
pub fn draw_x_mark(x: i32, y: i32, center: i32, size: i32, fg: &[u8; 4], bg: &[u8; 4]) -> [u8; 4] {
    let rx = x - center;
    let ry = y - center;
    let s = size as f32 / 32.0;
    let thick = (2.5 * s) as i32;
    let arm = (6.0 * s) as i32;

    let on_d1 = (rx - ry).abs() <= thick && rx.abs() <= arm && ry.abs() <= arm;
    let on_d2 = (rx + ry).abs() <= thick && rx.abs() <= arm && ry.abs() <= arm;

    if on_d1 || on_d2 {
        *fg
    } else {
        *bg
    }
}

/// Draw an exclamation mark (degraded indicator).
pub fn draw_exclamation(
    x: i32,
    y: i32,
    center: i32,
    size: i32,
    fg: &[u8; 4],
    bg: &[u8; 4],
) -> [u8; 4] {
    let rx = x - center;
    let ry = y - center;
    let s = size as f32 / 32.0;

    let bar_w = (2.5 * s) as i32;
    let bar_top = (-6.0 * s) as i32;
    let bar_bottom = (2.0 * s) as i32;
    let on_bar = rx.abs() <= bar_w && ry >= bar_top && ry <= bar_bottom;

    let dot_y = (5.0 * s) as i32;
    let dot_r = (2.0 * s) as i32;
    let dot_r_sq = dot_r * dot_r;
    let dy = ry - dot_y;
    let on_dot = rx * rx + dy * dy <= dot_r_sq;

    if on_bar || on_dot {
        *fg
    } else {
        *bg
    }
}

/// Icon type for mapping VPN state → icon variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconVariant {
    Connected,
    Connecting,
    Disconnected,
    Degraded,
    Failed,
}

impl IconVariant {
    /// ARGB colours: (background R, G, B, foreground R, G, B).
    pub fn colours(&self) -> (u8, u8, u8, u8, u8, u8) {
        match self {
            IconVariant::Connected => (46, 160, 67, 255, 255, 255),
            IconVariant::Connecting => (245, 158, 11, 255, 255, 255),
            IconVariant::Disconnected => (100, 116, 139, 255, 255, 255),
            IconVariant::Degraded => (249, 115, 22, 255, 255, 255),
            IconVariant::Failed => (239, 68, 68, 255, 255, 255),
        }
    }

    /// Standard icon sizes for multi-DPI support.
    pub fn sizes() -> &'static [i32] {
        &[16, 24, 32, 48]
    }

    /// Expected pixel-data length for one icon at the given size.
    pub fn data_len(size: i32) -> usize {
        (size * size * 4) as usize
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const FG: [u8; 4] = [255, 255, 255, 255];
    const BG: [u8; 4] = [255, 100, 100, 100];

    // --- draw_dots ---

    mod dots {
        use super::*;

        #[test]
        fn test_center_is_fg() {
            // Center dot should be foreground
            let pixel = draw_dots(16, 16, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_corner_is_bg() {
            // Far corner should be background
            let pixel = draw_dots(0, 0, 16, 32, &FG, &BG);
            assert_eq!(pixel, BG);
        }

        #[test]
        fn test_left_dot() {
            // Left dot at approximately (-6, 0) from center
            let pixel = draw_dots(10, 16, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_right_dot() {
            let pixel = draw_dots(22, 16, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }
    }

    // --- draw_dash ---

    mod dash {
        use super::*;

        #[test]
        fn test_center_is_fg() {
            let pixel = draw_dash(16, 16, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_corner_is_bg() {
            let pixel = draw_dash(0, 0, 16, 32, &FG, &BG);
            assert_eq!(pixel, BG);
        }

        #[test]
        fn test_horizontal_extent() {
            // Should be foreground near center-x
            let pixel = draw_dash(20, 16, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_vertical_out_of_range() {
            // Well above center
            let pixel = draw_dash(16, 5, 16, 32, &FG, &BG);
            assert_eq!(pixel, BG);
        }
    }

    // --- draw_x_mark ---

    mod x_mark {
        use super::*;

        #[test]
        fn test_center_is_fg() {
            let pixel = draw_x_mark(16, 16, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_corner_is_bg() {
            let pixel = draw_x_mark(0, 0, 16, 32, &FG, &BG);
            assert_eq!(pixel, BG);
        }

        #[test]
        fn test_diagonal_is_fg() {
            // On the forward diagonal (dx == dy)
            let pixel = draw_x_mark(19, 19, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }
    }

    // --- draw_exclamation ---

    mod exclamation {
        use super::*;

        #[test]
        fn test_center_bar_is_fg() {
            // Center of the bar
            let pixel = draw_exclamation(16, 14, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_dot_area() {
            // The dot is below center at y = center + 5*s ≈ 21 for size=32
            let pixel = draw_exclamation(16, 21, 16, 32, &FG, &BG);
            assert_eq!(pixel, FG);
        }

        #[test]
        fn test_corner_is_bg() {
            let pixel = draw_exclamation(0, 0, 16, 32, &FG, &BG);
            assert_eq!(pixel, BG);
        }
    }

    // --- IconVariant ---

    mod icon_variant {
        use super::*;

        #[test]
        fn test_colours_differ() {
            let c1 = IconVariant::Connected.colours();
            let c2 = IconVariant::Failed.colours();
            assert_ne!((c1.0, c1.1, c1.2), (c2.0, c2.1, c2.2));
        }

        #[test]
        fn test_sizes() {
            let sizes = IconVariant::sizes();
            assert_eq!(sizes.len(), 4);
            assert!(sizes.contains(&16));
            assert!(sizes.contains(&48));
        }

        #[test]
        fn test_data_len() {
            assert_eq!(IconVariant::data_len(16), 16 * 16 * 4);
            assert_eq!(IconVariant::data_len(32), 32 * 32 * 4);
        }

        #[test]
        fn test_all_variants_have_colours() {
            for variant in &[
                IconVariant::Connected,
                IconVariant::Connecting,
                IconVariant::Disconnected,
                IconVariant::Degraded,
                IconVariant::Failed,
            ] {
                let c = variant.colours();
                // FG should be white for all
                assert_eq!((c.3, c.4, c.5), (255, 255, 255));
            }
        }
    }
}
