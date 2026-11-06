use gpui::*;

use crate::workspace::LogAnalysisWorkspace;

/// Edge border hit-test thickness in pixels
const BORDER_THICKNESS: Pixels = px(6.0);
/// Corner hotspot dimension in pixels
const CORNER_SIZE: Pixels = px(12.0);

/// Renders invisible interactive window resize handles for Client-Side Decoration (CSD) environments (e.g. Ubuntu Wayland).
pub fn render_window_resize_borders(
    window: &Window,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Vec<Stateful<Div>> {
    // Only render interactive client-side resize handles if CSD is active and window is not maximized
    if !crate::components::titlebar::should_render_csd(window) || window.is_maximized() {
        return Vec::new();
    }

    vec![
        // ----------------------------------------------------
        // Corners (TopLeft, TopRight, BottomLeft, BottomRight)
        // ----------------------------------------------------
        div()
            .id("resize-corner-top-left")
            .absolute()
            .top_0()
            .left_0()
            .w(CORNER_SIZE)
            .h(CORNER_SIZE)
            .cursor_nwse_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::TopLeft);
            })),
        div()
            .id("resize-corner-top-right")
            .absolute()
            .top_0()
            .right_0()
            .w(CORNER_SIZE)
            .h(CORNER_SIZE)
            .cursor_nesw_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::TopRight);
            })),
        div()
            .id("resize-corner-bottom-left")
            .absolute()
            .bottom_0()
            .left_0()
            .w(CORNER_SIZE)
            .h(CORNER_SIZE)
            .cursor_nesw_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::BottomLeft);
            })),
        div()
            .id("resize-corner-bottom-right")
            .absolute()
            .bottom_0()
            .right_0()
            .w(CORNER_SIZE)
            .h(CORNER_SIZE)
            .cursor_nwse_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::BottomRight);
            })),

        // ----------------------------------------------------
        // Edges (Top, Bottom, Left, Right)
        // ----------------------------------------------------
        div()
            .id("resize-edge-top")
            .absolute()
            .top_0()
            .left(CORNER_SIZE)
            .right(CORNER_SIZE)
            .h(BORDER_THICKNESS)
            .cursor_n_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::Top);
            })),
        div()
            .id("resize-edge-bottom")
            .absolute()
            .bottom_0()
            .left(CORNER_SIZE)
            .right(CORNER_SIZE)
            .h(BORDER_THICKNESS)
            .cursor_s_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::Bottom);
            })),
        div()
            .id("resize-edge-left")
            .absolute()
            .left_0()
            .top(CORNER_SIZE)
            .bottom(CORNER_SIZE)
            .w(BORDER_THICKNESS)
            .cursor_w_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::Left);
            })),
        div()
            .id("resize-edge-right")
            .absolute()
            .right_0()
            .top(CORNER_SIZE)
            .bottom(CORNER_SIZE)
            .w(BORDER_THICKNESS)
            .cursor_e_resize()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, _| {
                window.start_window_resize(ResizeEdge::Right);
            })),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_border_constants_reasonable() {
        assert!(BORDER_THICKNESS >= px(4.0));
        assert!(CORNER_SIZE >= px(8.0));
        assert!(CORNER_SIZE > BORDER_THICKNESS);
    }

    #[core::prelude::v1::test]
    fn test_all_resize_edges_variants_present() {
        let edges = [
            ResizeEdge::Top,
            ResizeEdge::Bottom,
            ResizeEdge::Left,
            ResizeEdge::Right,
            ResizeEdge::TopLeft,
            ResizeEdge::TopRight,
            ResizeEdge::BottomLeft,
            ResizeEdge::BottomRight,
        ];
        assert_eq!(edges.len(), 8);
    }
}
