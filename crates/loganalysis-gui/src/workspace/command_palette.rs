use gpui::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    pub fn toggle_command_palette(&mut self, cx: &mut Context<Self>) {
        if self.is_command_palette_open {
            self.close_command_palette(cx);
        } else {
            self.open_command_palette(cx);
        }
    }

    pub fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        if self.filter_edit_modal.is_open {
            self.close_filter_modal(cx);
        }
        self.is_command_palette_open = true;
        self.palette_selected_index = 0;
        self.palette_input.update(cx, |input, cx| {
            input.clear(cx);
        });
        self.input_focus = InputFocusTarget::CommandPalette;
        cx.notify();
    }

    pub fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        self.is_command_palette_open = false;
        if self.input_focus == InputFocusTarget::CommandPalette {
            self.input_focus = InputFocusTarget::None;
        }
        cx.notify();
    }

    pub fn get_filtered_palette_commands(
        &self,
        query: &str,
    ) -> Vec<crate::modals::command_palette::PaletteCommand> {
        use crate::modals::command_palette::get_palette_commands;
        use loganalysis_core::fuzzy_match;

        let commands = get_palette_commands();
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return commands;
        }

        let mut scored: Vec<(crate::modals::command_palette::PaletteCommand, i32)> = commands
            .into_iter()
            .filter_map(|cmd| {
                fuzzy_match(trimmed, cmd.title).map(|score| (cmd, score))
            })
            .collect();

        scored.sort_by_key(|a| std::cmp::Reverse(a.1));
        scored.into_iter().map(|(cmd, _)| cmd).collect()
    }

    pub fn submit_command_palette(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let query = self.palette_input.read(cx).text().to_string();
        let matches = self.get_filtered_palette_commands(&query);
        if let Some(cmd) = matches.get(self.palette_selected_index) {
            let id = cmd.id;
            self.execute_palette_command(id, window, cx);
        } else {
            self.close_command_palette(cx);
        }
    }

    pub fn execute_palette_command(
        &mut self,
        cmd_id: crate::modals::command_palette::PaletteCommandId,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        use crate::modals::command_palette::PaletteCommandId;

        self.close_command_palette(cx);
        match cmd_id {
            PaletteCommandId::OpenFile => self.prompt_open_file(window.as_deref(), cx),
            PaletteCommandId::LoadFilters => self.prompt_load_filters(window.as_deref(), cx),
            PaletteCommandId::SaveFilters | PaletteCommandId::SaveFiltersAs => {
                self.prompt_save_filters(window.as_deref(), cx);
            }
            PaletteCommandId::Find => {
                self.toggle_search_bar(cx);
                if self.is_search_visible {
                    if let Some(win) = window.as_mut() {
                        win.focus(&self.find_input.read(cx).focus_handle);
                    }
                }
            }
            PaletteCommandId::ClearLogs => self.clear_logs(cx),
            PaletteCommandId::ToggleFilteredOnly => self.toggle_filtered_only(cx),
            PaletteCommandId::AddFilterRule => {
                self.open_add_filter_modal(cx);
                if let Some(win) = window.as_mut() {
                    win.focus(&self.modal_pattern_input.read(cx).focus_handle);
                }
            }
            PaletteCommandId::AddFilterTab => self.add_filter_tab(cx),
            PaletteCommandId::DeleteFilterTab => self.delete_current_tab(cx),
            PaletteCommandId::RenameFilterTab => self.rename_current_tab(cx),
            PaletteCommandId::StartAdbStream => self.start_adb_stream(cx),
            PaletteCommandId::StopAdbStream => self.stop_adb_stream(cx),
            PaletteCommandId::TogglePauseStream => self.toggle_pause_stream(cx),
            PaletteCommandId::ToggleAutoScroll => self.toggle_auto_scroll(cx),
            PaletteCommandId::RescanAdbDevices => self.refresh_adb_devices(cx),
            PaletteCommandId::ToggleFilterPanelPosition => self.toggle_filter_panel_position(cx),
            PaletteCommandId::ToggleFilterDock => self.toggle_filter_dock(cx),
            PaletteCommandId::ToggleFacetsBar => self.toggle_facets_bar(cx),
            PaletteCommandId::ToggleFullLineDisplay => self.toggle_full_line_display(cx),
            PaletteCommandId::ToggleTheme => self.toggle_theme(cx),
            PaletteCommandId::ZoomIn => self.zoom(1.0, cx),
            PaletteCommandId::ZoomOut => self.zoom(-1.0, cx),
            PaletteCommandId::ResetZoom => {
                self.font_size = px(12.0);
                self.status_text = "Font size reset to default (12px)".into();
                cx.notify();
            }
            PaletteCommandId::ToggleBookmark => self.toggle_bookmark_on_selected(cx),
            PaletteCommandId::NextBookmark => self.jump_next_bookmark(cx),
            PaletteCommandId::PrevBookmark => self.jump_prev_bookmark(cx),
            PaletteCommandId::ClearBookmarks => self.clear_all_bookmarks(cx),
            PaletteCommandId::ExportHtmlReport => {
                self.prompt_export_html_report(window.as_deref(), cx)
            }
            PaletteCommandId::ToggleBookmarkDrawer => self.toggle_bookmark_drawer(cx),
            PaletteCommandId::EditBookmarkNote => self.open_note_modal_for_selected(cx),
            PaletteCommandId::ToggleEpsTimeline => self.toggle_eps_timeline(cx),
            PaletteCommandId::ToggleSplitView => self.toggle_split_view(cx),
            PaletteCommandId::ToggleSplitOrientation => self.toggle_split_orientation(cx),
            PaletteCommandId::ToggleSyncedScroll => self.toggle_synced_scroll(cx),
            PaletteCommandId::OpenSecondaryLog => {
                self.prompt_open_secondary_file(window.as_deref(), cx)
            }
            PaletteCommandId::ToggleColumnView => self.toggle_column_view(cx),
            PaletteCommandId::OpenMergedLogs => {
                self.prompt_open_merged_files(window.as_deref(), cx)
            }
        }
    }
}
