#![recursion_limit = "2048"]

pub mod actions;
pub mod cli;
pub mod components;
pub mod controllers;
pub mod dialogs;
pub mod logging;
pub mod modals;
pub mod selection;
pub mod theme;
pub mod types;
pub mod workspace;

use gpui::prelude::FluentBuilder;
use gpui::*;

use actions::*;
use components::activity_bar::render_activity_bar;
use components::filter_dock::{render_filter_dock, render_filter_dock_splitter};
use components::log_view::render_log_viewport;
use components::resize_border::render_window_resize_borders;
use components::status_bar::render_status_bar;
use components::timeline_sparkline::{render_timeline_sparkline, should_render_timeline_ribbon};
use components::toolbar::render_unified_header;
use modals::bookmark_drawer::render_bookmark_drawer;
use modals::command_palette::render_command_palette;
use modals::filter_edit_modal::render_filter_edit_modal;
use modals::note_edit_modal::render_note_edit_modal;
use types::{FilterPanelPosition, InputFocusTarget};
pub use workspace::LogAnalysisWorkspace;

/// Returns whether the left Activity Bar is visible for the given Zen mode state.
pub fn workspace_shows_activity_bar(is_zen_mode: bool) -> bool {
    !is_zen_mode
}

/// Returns whether the bottom Status Bar is visible for the given Zen mode state.
pub fn workspace_shows_status_bar(is_zen_mode: bool) -> bool {
    !is_zen_mode
}

/// Returns whether the Filter Dock is visible for the given Zen mode and dock visibility flags.
pub fn workspace_shows_filter_dock(is_zen_mode: bool, is_filter_dock_visible: bool) -> bool {
    !is_zen_mode && is_filter_dock_visible
}

macro_rules! define_default_keybindings {
    ($(($key:literal, $action:path)),* $(,)?) => {
        /// Returns keystroke to canonical action name contract entries used for global shortcuts.
        pub fn default_keybinding_specs() -> &'static [(&'static str, &'static str)] {
            static SPECS: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
                std::sync::OnceLock::new();
            SPECS
                .get_or_init(|| vec![$(($key, gpui::Action::name(&$action))),*])
                .as_slice()
        }

        /// Registers all default global GPUI key bindings.
        pub fn register_global_keybindings(cx: &mut App) {
            cx.bind_keys([
                $(KeyBinding::new($key, $action, None)),*
            ]);
        }
    };
}

define_default_keybindings!(
    ("ctrl-o", OpenFile),
    ("cmd-o", OpenFile),
    ("ctrl-h", ToggleFilteredOnly),
    ("cmd-h", ToggleFilteredOnly),
    ("ctrl-f", Find),
    ("cmd-f", Find),
    ("f3", FindNext),
    ("shift-f3", FindPrev),
    ("ctrl-c", CopySelection),
    ("cmd-c", CopySelection),
    ("ctrl-shift-f", AddFilter),
    ("cmd-shift-f", AddFilter),
    ("ctrl-s", SaveFilters),
    ("cmd-s", SaveFilters),
    ("ctrl-shift-s", SaveFiltersAs),
    ("cmd-shift-s", SaveFiltersAs),
    ("ctrl-shift-o", LoadFilters),
    ("cmd-shift-o", LoadFilters),
    ("ctrl-k", ClearLogs),
    ("cmd-k", ClearLogs),
    ("ctrl-=", ZoomIn),
    ("cmd-=", ZoomIn),
    ("ctrl-+", ZoomIn),
    ("cmd-+", ZoomIn),
    ("ctrl--", ZoomOut),
    ("cmd--", ZoomOut),
    ("ctrl-0", ResetZoom),
    ("cmd-0", ResetZoom),
    ("ctrl-space", TogglePauseStream),
    ("ctrl-t", AddFilterTab),
    ("cmd-t", AddFilterTab),
    ("ctrl-d", DeleteFilterTab),
    ("cmd-d", DeleteFilterTab),
    ("ctrl-r", RenameFilterTab),
    ("cmd-r", RenameFilterTab),
    ("ctrl-shift-l", ToggleFilterPanelPosition),
    ("cmd-shift-l", ToggleFilterPanelPosition),
    ("ctrl-p", ToggleCommandPalette),
    ("cmd-p", ToggleCommandPalette),
    ("ctrl-shift-p", ToggleCommandPalette),
    ("cmd-shift-p", ToggleCommandPalette),
    ("f2", ToggleBookmark),
    ("shift-f2", PrevBookmark),
    ("ctrl-f2", ToggleBookmark),
    ("alt-f2", NextBookmark),
    ("ctrl-b", ToggleBookmarkDrawer),
    ("cmd-b", ToggleBookmarkDrawer),
    ("alt-enter", EditBookmarkNote),
    ("ctrl-shift-t", ToggleEpsTimeline),
    ("cmd-shift-t", ToggleEpsTimeline),
    ("f11", ToggleZenMode),
    ("ctrl-shift-z", ToggleZenMode),
    ("cmd-shift-z", ToggleZenMode),
    ("ctrl-\\", ToggleSplitView),
    ("cmd-\\", ToggleSplitView),
    ("ctrl-alt-\\", ToggleSplitOrientation),
    ("cmd-alt-\\", ToggleSplitOrientation),
    ("ctrl-alt-s", ToggleSyncedScroll),
    ("cmd-alt-s", ToggleSyncedScroll),
    ("ctrl-g", ToggleColumnView),
    ("cmd-g", ToggleColumnView),
    ("ctrl-shift-m", OpenMergedLogs),
    ("cmd-shift-m", OpenMergedLogs),
    ("ctrl-j", ToggleFilterDock),
    ("cmd-j", ToggleFilterDock),
    ("alt-f", ToggleFacetsBar),
    ("ctrl-shift-r", StartAdb),
    ("cmd-shift-r", StartAdb),
    ("ctrl-u", ToggleTheme),
    ("cmd-u", ToggleTheme),
    ("f10", ToggleTheme),
    ("alt-z", ToggleFullLineDisplay),
    ("ctrl-shift-d", ToggleLineDetail),
    ("cmd-shift-d", ToggleLineDetail),
);

impl Render for LogAnalysisWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !cx.has_active_drag()
            && (self.dragging_filter_rule_index.is_some() || self.filter_drag_drop_target.is_some())
        {
            self.clear_filter_drag_state_fields();
        }
        if self.is_line_detail_open {
            self.ensure_line_detail_cache();
        }

        let modal_pattern_focused = self
            .modal_pattern_input
            .read(cx)
            .focus_handle
            .is_focused(window);
        let modal_bg_hex_focused = self
            .modal_bg_hex_input
            .read(cx)
            .focus_handle
            .is_focused(window);
        let modal_fg_hex_focused = self
            .modal_fg_hex_input
            .read(cx)
            .focus_handle
            .is_focused(window);
        let note_input_focused = self.note_input.read(cx).focus_handle.is_focused(window);
        let find_input_focused = self.find_input.read(cx).focus_handle.is_focused(window);
        let palette_input_focused = self.palette_input.read(cx).focus_handle.is_focused(window);
        let filter_rule_focused = self
            .filter_rule_input
            .read(cx)
            .focus_handle
            .is_focused(window);

        if self.filter_edit_modal.is_open {
            match self.input_focus {
                InputFocusTarget::ModalPattern if !modal_pattern_focused => {
                    window.focus(&self.modal_pattern_input.read(cx).focus_handle);
                }
                InputFocusTarget::ModalBgHex if !modal_bg_hex_focused => {
                    window.focus(&self.modal_bg_hex_input.read(cx).focus_handle);
                }
                InputFocusTarget::ModalFgHex if !modal_fg_hex_focused => {
                    window.focus(&self.modal_fg_hex_input.read(cx).focus_handle);
                }
                _ => {}
            }
        } else if self.bookmarks.is_note_modal_open {
            if self.input_focus == InputFocusTarget::BookmarkNote && !note_input_focused {
                window.focus(&self.note_input.read(cx).focus_handle);
            }
        } else if self.is_command_palette_open {
            if !palette_input_focused {
                window.focus(&self.palette_input.read(cx).focus_handle);
            }
        } else if self.search_ctrl.is_visible && self.input_focus == InputFocusTarget::SearchBar {
            if !find_input_focused {
                window.focus(&self.find_input.read(cx).focus_handle);
            }
        } else if !modal_pattern_focused
            && !modal_bg_hex_focused
            && !modal_fg_hex_focused
            && !note_input_focused
            && !find_input_focused
            && !palette_input_focused
            && !filter_rule_focused
            && !self.focus_handle.is_focused(window)
        {
            window.focus(&self.focus_handle);
        }

        let theme = self.theme;
        let is_bottom_dock = self.filter_panel_position == FilterPanelPosition::Bottom;
        let is_csd = components::titlebar::should_render_csd(window);
        let is_floating = !window.is_maximized();

        let unified_header_el = render_unified_header(self, is_csd, cx);
        let timeline_ribbon_el =
            if should_render_timeline_ribbon(self.is_timeline_visible, self.is_zen_mode) {
                render_timeline_sparkline(self, cx)
            } else {
                None
            };
        let activity_bar_el = render_activity_bar(self, cx);
        let filter_panel_el = render_filter_dock(self, cx);
        let filter_splitter_el = render_filter_dock_splitter(self, cx);
        let log_viewport_el = render_log_viewport(self, cx);
        let status_bar_el = render_status_bar(self, cx);
        let modal_el = render_filter_edit_modal(self, cx);
        let palette_el = render_command_palette(self, cx);
        let bookmark_drawer_el = render_bookmark_drawer(self, cx);
        let note_modal_el = render_note_edit_modal(self, cx);
        let resize_borders = render_window_resize_borders(is_csd, window, cx);

        let content_area =
            if workspace_shows_filter_dock(self.is_zen_mode, self.is_filter_dock_visible) {
                if !is_bottom_dock {
                    div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_row()
                        .overflow_hidden()
                        .when(self.is_resizing_filter_dock, |s| s.cursor_col_resize())
                        .child(filter_panel_el)
                        .child(filter_splitter_el)
                        .child(log_viewport_el)
                } else {
                    div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .when(self.is_resizing_filter_dock, |s| s.cursor_row_resize())
                        .child(log_viewport_el)
                        .child(filter_splitter_el)
                        .child(filter_panel_el)
                }
            } else {
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .overflow_hidden()
                    .child(log_viewport_el)
            };

        let body = div()
            .flex_1()
            .w_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .when(workspace_shows_activity_bar(self.is_zen_mode), |s| {
                s.child(activity_bar_el)
            })
            .child(content_area);

        div()
            .id("workspace-container")
            .track_focus(&self.focus_handle)
            .when(self.is_resizing_line_detail, |s| s.cursor_row_resize())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if event.dragging() {
                    if this.is_resizing_filter_dock {
                        this.update_resizing_filter_dock(event.position, cx);
                    }
                    if this.is_resizing_line_detail {
                        this.update_resizing_line_detail(event.position, cx);
                    }
                    if this.is_dragging_scrollbar {
                        this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                    }
                    if this.is_dragging_secondary_scrollbar {
                        this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
                    }
                    if this.is_dragging_h_scrollbar {
                        this.scroll_h_to_mouse_x(true, event.position.x, cx);
                    }
                    if this.is_dragging_secondary_h_scrollbar {
                        this.scroll_h_to_mouse_x(false, event.position.x, cx);
                    }
                } else {
                    if this.dragging_filter_rule_index.is_some() {
                        if cx.has_active_drag() {
                            cx.stop_active_drag(window);
                        }
                        this.clear_filter_drag_state(cx);
                    }
                    if this.is_resizing_filter_dock {
                        this.stop_resizing_filter_dock(cx);
                    }
                    if this.is_resizing_line_detail {
                        this.stop_resizing_line_detail(cx);
                    }
                    if this.clear_scrollbar_drag_flags() {
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.is_resizing_filter_dock {
                        this.stop_resizing_filter_dock(cx);
                    }
                    if this.is_resizing_line_detail {
                        this.stop_resizing_line_detail(cx);
                    }
                    if this.clear_scrollbar_drag_flags() {
                        cx.notify();
                    }
                    this.clear_filter_drag_state(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.is_resizing_filter_dock {
                        this.stop_resizing_filter_dock(cx);
                    }
                    if this.is_resizing_line_detail {
                        this.stop_resizing_line_detail(cx);
                    }
                    if this.clear_scrollbar_drag_flags() {
                        cx.notify();
                    }
                    this.clear_filter_drag_state(cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    if this.dragging_filter_rule_index.is_some()
                        || this.filter_drag_drop_target.is_some()
                    {
                        if cx.has_active_drag() {
                            cx.stop_active_drag(window);
                        }
                        this.clear_filter_drag_state(cx);
                        return;
                    }
                    if this.handle_escape(cx) {
                        return;
                    }
                }
                this.handle_key_down(event, window, cx);
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                if let Some(path) = paths.paths().first() {
                    this.open_file(path, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFile, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.prompt_open_file(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilteredOnly, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_filtered_only(cx);
            }))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_search_bar(cx);
                if this.search_ctrl.is_visible {
                    window.focus(&this.find_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.search(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.search(false, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseFind, _window, cx| {
                this.close_search_bar(cx);
            }))
            .on_action(cx.listener(|this, _: &CopySelection, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.copy_selection(cx);
            }))
            .on_action(cx.listener(|this, _: &AddFilter, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.open_add_filter_modal(cx);
                window.focus(&this.modal_pattern_input.read(cx).focus_handle);
            }))
            .on_action(cx.listener(|this, _: &SaveFilters, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.save_filters(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &SaveFiltersAs, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.prompt_save_filters(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &LoadFilters, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.prompt_load_filters(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ClearLogs, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.clear_logs(cx);
            }))
            .on_action(cx.listener(|this, _: &AddFilterTab, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.add_filter_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteFilterTab, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.delete_current_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &RenameFilterTab, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.rename_current_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_theme(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFullLineDisplay, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_full_line_display(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleLineDetail, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_line_detail(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleAutoScroll, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_auto_scroll(cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.zoom(1.0, cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.zoom(-1.0, cx);
            }))
            .on_action(cx.listener(|this, _: &ResetZoom, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.reset_zoom(cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePauseStream, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.toggle_pause_stream(cx);
            }))
            .on_action(
                cx.listener(|this, _: &AddIncludeFromSelection, _window, cx| {
                    if this.has_blocking_modal_open() {
                        return;
                    }
                    this.add_quick_filter_from_selection(false, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &AddExcludeFromSelection, _window, cx| {
                    if this.has_blocking_modal_open() {
                        return;
                    }
                    this.add_quick_filter_from_selection(true, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &ToggleFilterPanelPosition, _window, cx| {
                    if this.has_blocking_modal_open() {
                        return;
                    }
                    this.toggle_filter_panel_position(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                if this.filter_edit_modal.is_open || this.bookmarks.is_note_modal_open {
                    return;
                }
                this.toggle_command_palette(cx);
                if this.is_command_palette_open {
                    window.focus(&this.palette_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleBookmark, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_bookmark_on_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &NextBookmark, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.jump_next_bookmark(cx);
            }))
            .on_action(cx.listener(|this, _: &PrevBookmark, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.jump_prev_bookmark(cx);
            }))
            .on_action(cx.listener(|this, _: &ClearPatternHistory, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.clear_pattern_history(cx);
            }))
            .on_action(cx.listener(|this, _: &ClearBookmarks, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.clear_all_bookmarks(cx);
            }))
            .on_action(cx.listener(|this, _: &ExportHtmlReport, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.prompt_export_html_report(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleBookmarkDrawer, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_bookmark_drawer(cx);
            }))
            .on_action(cx.listener(|this, _: &EditBookmarkNote, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.open_note_modal_for_selected(cx);
                if this.bookmarks.is_note_modal_open {
                    window.focus(&this.note_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleEpsTimeline, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_eps_timeline(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleZenMode, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_zen_mode(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSplitView, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_split_view(cx);
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSplitOrientation, _window, cx| {
                    if this.has_blocking_modal_open() {
                        return;
                    }
                    this.toggle_split_orientation(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &ToggleSyncedScroll, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_synced_scroll(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSecondaryLog, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.prompt_open_secondary_file(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &OpenMergedLogs, window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.prompt_open_merged_files(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleColumnView, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.toggle_column_view(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilterDock, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.toggle_filter_dock(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFacetsBar, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                this.toggle_facets_bar(cx);
            }))
            .on_action(cx.listener(|this, _: &StartAdb, window, cx| {
                if this.has_blocking_modal_open() || this.has_any_text_input_focus(window, cx) {
                    return;
                }
                if this.adb.is_streaming {
                    this.stop_adb_stream(cx);
                } else {
                    this.start_adb_stream(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &StopAdb, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.stop_adb_stream(cx);
            }))
            .on_action(cx.listener(|this, _: &RefreshAdbDevices, _window, cx| {
                if this.has_blocking_modal_open() {
                    return;
                }
                this.refresh_adb_devices(cx);
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_dark)
            .text_color(theme.text_primary)
            .when(is_csd && is_floating, |s| {
                s.border_1().border_color(theme.border)
            })
            .child(unified_header_el)
            .children(timeline_ribbon_el)
            .child(body)
            .when(workspace_shows_status_bar(self.is_zen_mode), |s| {
                s.child(status_bar_el)
            })
            .children(modal_el)
            .children(palette_el)
            .children(bookmark_drawer_el)
            .children(note_modal_el)
            .children(resize_borders)
    }
}

/// Builds the sanitized `.desktop` entry file content for `exec` and `icon_path`.
/// Returns `None` when either sanitized path is empty.
pub fn build_desktop_entry_content(
    exec: &std::path::Path,
    icon_path: &std::path::Path,
) -> Option<String> {
    let safe_exec = exec.to_string_lossy().replace(['\n', '\r', '"', '\\'], "");
    let safe_icon = icon_path
        .to_string_lossy()
        .replace(['\n', '\r', '"', '\\'], "");
    if safe_exec.is_empty() || safe_icon.is_empty() {
        return None;
    }
    Some(format!(
        "[Desktop Entry]\n\
        Name=LogAnalysis\n\
        Comment=GPU-Accelerated High Performance Log Analysis Tool\n\
        Exec=\"{}\" %F\n\
        Icon={}\n\
        Terminal=false\n\
        Type=Application\n\
        Categories=Development;Utility;System;\n\
        MimeType=text/plain;application/json;\n\
        StartupWMClass=loganalysis\n",
        safe_exec, safe_icon
    ))
}

/// Writes `bytes` to `path` only if the file is missing or its current contents differ.
/// Returns `Ok(true)` if the file was created or updated, or `Ok(false)` when already identical.
pub fn write_if_changed(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<bool> {
    if let Ok(existing) = std::fs::read(path) {
        if existing == bytes {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Ensures desktop entry and application icon are installed on Linux desktops.
#[cfg(target_os = "linux")]
pub fn ensure_desktop_integration() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    if home.is_empty() {
        return;
    }
    let home_path = std::path::PathBuf::from(home);
    let apps_dir = home_path.join(".local/share/applications");
    let icons_dir = home_path.join(".local/share/icons/hicolor/256x256/apps");
    let desktop_file = apps_dir.join("loganalysis.desktop");
    let icon_file = icons_dir.join("loganalysis.png");

    let _ = std::fs::create_dir_all(&apps_dir);
    let _ = std::fs::create_dir_all(&icons_dir);

    // 1. Write icon only when missing or content changed
    const ICON_BYTES: &[u8] = include_bytes!("../../../assets/loganalysis.png");
    let icon_changed = write_if_changed(&icon_file, ICON_BYTES).unwrap_or(false);

    // 2. Resolve executable path (AppImage or binary) and write desktop file only when changed
    let exec_path = std::env::var("APPIMAGE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok());

    let desktop_changed = exec_path
        .as_deref()
        .and_then(|exec| build_desktop_entry_content(exec, &icon_file))
        .and_then(|content| write_if_changed(&desktop_file, content.as_bytes()).ok())
        .unwrap_or(false);

    // 3. Refresh desktop caches off the startup thread only when files actually changed
    if icon_changed || desktop_changed {
        let hicolor_dir = home_path.join(".local/share/icons/hicolor");
        let _ = std::thread::Builder::new()
            .name("desktop-integration-refresh".to_string())
            .spawn(move || {
                let _ = std::process::Command::new("update-desktop-database")
                    .arg(&apps_dir)
                    .status();
                let _ = std::process::Command::new("gtk-update-icon-cache")
                    .args(["-f", "-t"])
                    .arg(&hicolor_dir)
                    .status();
            });
    }
}

/// Detects initial window decoration mode based on compositor session and explicit overrides.
/// On Linux Wayland (e.g. GNOME Mutter), Client-Side Decorations (CSD) must be explicitly requested
/// because the compositor does not provide server-side window decorations.
pub fn detect_initial_window_decorations() -> Option<WindowDecorations> {
    if let Some(forced) = components::titlebar::check_csd_env_override() {
        return if forced {
            Some(WindowDecorations::Client)
        } else {
            Some(WindowDecorations::Server)
        };
    }

    #[cfg(target_os = "linux")]
    {
        if dialogs::is_wayland_session() {
            Some(WindowDecorations::Client)
        } else {
            None
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Returns `true` when `XMODIFIERS` should be removed at startup to bypass `zed-xim` on X11.
pub(crate) fn should_sanitize_xmodifiers(
    keep_xmodifiers: Option<&std::ffi::OsStr>,
    xmodifiers: Option<&std::ffi::OsStr>,
) -> bool {
    keep_xmodifiers.is_none() && xmodifiers.is_some()
}

/// Launches the LogAnalysis GPUI desktop application.
pub fn run_app() {
    // Evaluate CLI flags early before crash handler, desktop integration, or GPU window creation
    let cli_paths = match cli::parse_cli_args(std::env::args().skip(1)) {
        cli::CliAction::PrintVersion => {
            println!("LogAnalysis {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        cli::CliAction::PrintHelp => {
            println!("{}", cli::help_text());
            return;
        }
        cli::CliAction::UnknownFlag(flag) => {
            eprintln!("Unknown option: {flag}\n\n{}", cli::help_text());
            std::process::exit(2);
        }
        cli::CliAction::OpenPaths(paths) => paths,
    };

    logging::init_crash_handler();

    // Guard against upstream Zed/GPUI bug on Linux X11:
    // When CJK/Korean IMEs (e.g. ibus, fcitx) send PreeditDraw compound text events,
    // zed-xim panics with 'Encoding Error: UnsupportedEncoding' at client.rs:237.
    // Unsetting XMODIFIERS safely bypasses zed-xim in favor of GPUI's native XKB input handler.
    // Must execute while single-threaded, before `ensure_desktop_integration()` spawns
    // `"desktop-integration-refresh"` (which forks subprocesses reading `environ`).
    if should_sanitize_xmodifiers(
        std::env::var_os("LOGANALYSIS_KEEP_XMODIFIERS").as_deref(),
        std::env::var_os("XMODIFIERS").as_deref(),
    ) {
        logging::log_event(
            "INFO",
            "Sanitized XMODIFIERS to prevent upstream zed-xim PreeditDraw UnsupportedEncoding panic on X11",
        );
        std::env::remove_var("XMODIFIERS");
    }

    #[cfg(target_os = "linux")]
    ensure_desktop_integration();

    logging::log_event("INFO", "LogAnalysis starting up");

    Application::new().run(move |cx: &mut App| {
        // Register Global Key Bindings
        register_global_keybindings(cx);

        // Register Scoped TextInput Key Bindings (Cut, Copy, Paste, Arrows, Selection, etc.)
        components::text_input::register_keybindings(cx);

        let window_decorations = detect_initial_window_decorations();

        let bounds = Bounds::centered(None, size(px(1200.0), px(850.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("loganalysis".into()),
            titlebar: Some(TitlebarOptions {
                title: Some("LogAnalysis (High Performance)".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_decorations,
            ..Default::default()
        };

        let initial_paths = cli_paths.clone();
        if let Err(e) = cx.open_window(options, move |window, cx| {
            let entity = cx.new(LogAnalysisWorkspace::new);
            entity.update(cx, |workspace, _cx| {
                window.focus(&workspace.focus_handle);
            });
            if initial_paths.is_empty() {
                // Auto-scan ADB devices on startup in background when no initial file argument is provided
                entity.update(cx, |workspace, cx| {
                    workspace.refresh_adb_devices(cx);
                });
            } else if initial_paths.len() == 1 {
                entity.update(cx, |workspace, cx| {
                    workspace.open_file(&initial_paths[0], cx);
                });
            } else {
                entity.update(cx, |workspace, cx| {
                    workspace.open_merged_files(&initial_paths, cx);
                });
            }
            entity
        }) {
            logging::log_event(
                "ERROR",
                &format!("Failed to open LogAnalysis window: {:?}", e),
            );
        }
    });
    logging::flush_logs();
}

#[cfg(test)]
mod tests {
    use super::{build_desktop_entry_content, should_sanitize_xmodifiers, write_if_changed};
    use std::ffi::OsStr;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_build_desktop_entry_content_sanitizes_and_validates() {
        let content = build_desktop_entry_content(
            Path::new("/usr/local/bin/loganalysis"),
            Path::new("/home/user/.local/share/icons/loganalysis.png"),
        )
        .expect("valid paths should produce desktop entry");
        assert!(content.contains("Exec=\"/usr/local/bin/loganalysis\" %F\n"));
        assert!(content.contains("Icon=/home/user/.local/share/icons/loganalysis.png\n"));
        assert!(content.contains("StartupWMClass=loganalysis\n"));

        let sanitized = build_desktop_entry_content(
            Path::new("/opt/\"evil\"\\\napp\r"),
            Path::new("/home/\"user\"/icon\\\n.png"),
        )
        .expect("sanitized paths should still produce desktop entry");
        assert!(sanitized.contains("Exec=\"/opt/evilapp\" %F\n"));
        assert!(sanitized.contains("Icon=/home/user/icon.png\n"));

        assert!(
            build_desktop_entry_content(Path::new("\"\n\r\\"), Path::new("/icon.png")).is_none()
        );
        assert!(
            build_desktop_entry_content(Path::new("/bin/app"), Path::new("\"\n\r\\")).is_none()
        );
    }

    #[test]
    fn test_write_if_changed_skips_identical_and_writes_diffs() {
        let dir = TempDir::new().expect("create temp dir");
        let target = dir.path().join("nested/loganalysis.desktop");

        assert!(write_if_changed(&target, b"v1").unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"v1");

        assert!(!write_if_changed(&target, b"v1").unwrap());
        assert!(write_if_changed(&target, b"v2").unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"v2");
    }

    #[test]
    fn test_should_sanitize_xmodifiers_matrix() {
        assert!(!should_sanitize_xmodifiers(None, None));
        assert!(should_sanitize_xmodifiers(
            None,
            Some(OsStr::new("@im=ibus"))
        ));
        assert!(!should_sanitize_xmodifiers(Some(OsStr::new("1")), None));
        assert!(!should_sanitize_xmodifiers(
            Some(OsStr::new("1")),
            Some(OsStr::new("@im=fcitx"))
        ));
    }

    #[test]
    fn test_default_keybinding_specs_macos_cmd_parity() {
        use super::actions::*;
        use super::default_keybinding_specs;
        use gpui::Action;

        let specs = default_keybinding_specs();
        let expected_cmd_bindings = [
            ("cmd-o", Action::name(&OpenFile)),
            ("cmd-h", Action::name(&ToggleFilteredOnly)),
            ("cmd-f", Action::name(&Find)),
            ("cmd-c", Action::name(&CopySelection)),
            ("cmd-shift-f", Action::name(&AddFilter)),
            ("cmd-s", Action::name(&SaveFilters)),
            ("cmd-shift-s", Action::name(&SaveFiltersAs)),
            ("cmd-shift-o", Action::name(&LoadFilters)),
            ("cmd-k", Action::name(&ClearLogs)),
            ("cmd-=", Action::name(&ZoomIn)),
            ("cmd-+", Action::name(&ZoomIn)),
            ("cmd--", Action::name(&ZoomOut)),
            ("cmd-t", Action::name(&AddFilterTab)),
            ("cmd-d", Action::name(&DeleteFilterTab)),
            ("cmd-r", Action::name(&RenameFilterTab)),
            ("cmd-shift-l", Action::name(&ToggleFilterPanelPosition)),
            ("cmd-alt-\\", Action::name(&ToggleSplitOrientation)),
            ("cmd-alt-s", Action::name(&ToggleSyncedScroll)),
        ];

        for pair in expected_cmd_bindings {
            assert!(
                specs.contains(&pair),
                "Missing macOS keybinding parity spec: {:?}",
                pair
            );
        }
    }
}
