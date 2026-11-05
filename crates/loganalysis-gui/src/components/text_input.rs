use std::ops::Range;
use gpui::{
    actions, div, fill, hsla, point, prelude::*, px, relative, rgba, size,
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Submit,
        Cancel,
    ]
);

/// Events emitted by TextInput upon key submission or cancellation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    Submit,
    Cancel,
    Change,
}

/// Registers OS-standard keybindings scoped to the `TextInput` key context
pub fn register_keybindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("ctrl-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("ctrl-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("ctrl-x", Cut, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("enter", Submit, Some("TextInput")),
        KeyBinding::new("escape", Cancel, Some("TextInput")),
    ]);
}

/// Converts a UTF-16 range relative to a string slice `s` into a byte offset range `Range<usize>`,
/// ensuring the resulting range aligns with valid UTF-8 character boundaries.
pub fn utf16_range_to_utf8_in_str(s: &str, range_utf16: &Range<usize>) -> Range<usize> {
    let mut utf8_start = 0;
    let mut utf8_end = 0;
    let mut utf16_count = 0;

    for ch in s.chars() {
        if utf16_count < range_utf16.start {
            utf8_start += ch.len_utf8();
        }
        if utf16_count < range_utf16.end {
            utf8_end += ch.len_utf8();
        }
        utf16_count += ch.len_utf16();
    }
    if utf16_count < range_utf16.end {
        utf8_end = s.len();
    }
    utf8_start..utf8_end
}

impl EventEmitter<TextInputEvent> for TextInput {}

/// A native-grade GPUI text input component supporting full OS IME (Hangul/CJK),
/// selection, clipboard, and horizontal scrolling.
pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub pivot: usize,
    pub marked_range: Option<Range<usize>>,
    pub last_layout: Option<ShapedLine>,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub is_selecting: bool,
    pub scroll_offset: Pixels,
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            pivot: 0,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_offset: px(0.0),
        }
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    pub fn set_text(&mut self, new_text: &str, cx: &mut Context<Self>) {
        self.content = new_text.to_string().into();
        let len = self.content.len();
        self.selected_range = len..len;
        self.selection_reversed = false;
        self.pivot = len;
        self.marked_range = None;
        self.scroll_offset = px(0.0);
        cx.emit(TextInputEvent::Change);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_text("", cx);
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.pivot = offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if offset < self.pivot {
            self.selected_range = offset..self.pivot;
            self.selection_reversed = true;
        } else {
            self.selected_range = self.pivot..offset;
            self.selection_reversed = false;
        }
        cx.notify();
    }

    fn select_word_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.content.is_empty() {
            return;
        }
        self.selected_range = find_word_range_at(&self.content, offset);
        self.pivot = self.selected_range.start;
        self.selection_reversed = false;
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref()) else {
            return 0;
        };
        let rel_x = (position.x - bounds.left() + self.scroll_offset).max(px(0.0));
        line.closest_index_for_x(rel_x).min(self.content.len())
    }

    // Action Handlers
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;
        let index = self.index_for_mouse_position(event.position);
        self.pivot = index;

        if event.click_count == 2 {
            self.select_word_at(index, cx);
        } else if event.click_count >= 3 {
            self.selected_range = 0..self.content.len();
            self.pivot = 0;
            self.selection_reversed = false;
            cx.notify();
        } else if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let index = self.index_for_mouse_position(event.position);
            self.select_to(index, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(clipboard) = cx.read_from_clipboard() {
            if let Some(text) = clipboard.text() {
                let clean = text.replace(['\r', '\n'], "");
                self.replace_text_in_range(None, &clean, window, cx);
            }
        }
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let text = self.content[self.selected_range.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let text = self.content[self.selected_range.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.replace_text_in_range(None, "", window, cx);
        }
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        if start <= end && self.content.is_char_boundary(start) && self.content.is_char_boundary(end) {
            Some(self.content[start..end].to_string())
        } else {
            None
        }
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());

        self.content =
            (self.content[0..start].to_owned() + new_text + &self.content[end..]).into();
        self.selected_range = start + new_text.len()..start + new_text.len();
        self.pivot = start + new_text.len();
        self.selection_reversed = false;
        self.marked_range.take();
        cx.emit(TextInputEvent::Change);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());

        self.content =
            (self.content[0..start].to_owned() + new_text + &self.content[end..]).into();

        if !new_text.is_empty() {
            self.marked_range = Some(start..start + new_text.len());
        } else {
            self.marked_range = None;
        }

        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| utf16_range_to_utf8_in_str(new_text, r))
            .map(|new_r| (start + new_r.start)..(start + new_r.end))
            .unwrap_or_else(|| (start + new_text.len())..(start + new_text.len()));
        let sel_start = self.selected_range.start.min(self.content.len());
        let sel_end = self.selected_range.end.min(self.content.len());
        self.selected_range = sel_start..sel_end;
        self.pivot = self.selected_range.end;
        self.selection_reversed = false;

        cx.emit(TextInputEvent::Change);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start) - self.scroll_offset,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end) - self.scroll_offset,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let last_layout = self.last_layout.as_ref()?;
        let rel_x = point.x - bounds.left() + self.scroll_offset;
        let utf8_index = last_layout.closest_index_for_x(rel_x);
        Some(self.offset_to_utf16(utf8_index))
    }
}

pub struct TextElement {
    input: Entity<TextInput>,
}

pub struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_offset: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0., 0.35))
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end.saturating_sub(marked_range.start),
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.5),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len().saturating_sub(marked_range.end),
                    ..run
                },
            ]
            .into_iter()
            .filter(|r| r.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_x = line.x_for_index(cursor);
        let viewport_w = bounds.size.width;
        let margin = px(28.0);
        let mut scroll = input.scroll_offset;

        if cursor_x - scroll < margin {
            scroll = (cursor_x - margin).max(px(0.0));
        } else if cursor_x - scroll > viewport_w - margin {
            scroll = cursor_x - viewport_w + margin;
        }

        let max_scroll = (line.width - viewport_w + margin).max(px(0.0));
        if scroll > max_scroll {
            scroll = max_scroll;
        }

        let (selection, cursor_quad) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x - scroll, bounds.top()),
                        size(px(2.0), bounds.bottom() - bounds.top()),
                    ),
                    style.color,
                )),
            )
        } else {
            let sel_start = line.x_for_index(selected_range.start);
            let sel_end = line.x_for_index(selected_range.end);
            let min_x = sel_start.min(sel_end);
            let max_x = sel_start.max(sel_end);
            (
                Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + min_x - scroll, bounds.top()),
                        point(bounds.left() + max_x - scroll, bounds.bottom()),
                    ),
                    rgba(0x3388ff44),
                )),
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x - scroll, bounds.top()),
                        size(px(2.0), bounds.bottom() - bounds.top()),
                    ),
                    style.color,
                )),
            )
        };

        PrepaintState {
            line: Some(line),
            cursor: cursor_quad,
            selection,
            scroll_offset: scroll,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        let scroll = prepaint.scroll_offset;

        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let line = prepaint.line.take().unwrap();
        let origin = point(bounds.origin.x - scroll, bounds.origin.y);
        line.paint(origin, window.line_height(), window, cx).unwrap();

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_offset = scroll;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .w_full()
            .h_full()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .overflow_hidden()
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(|this, _: &Submit, _window, cx| {
                // If IME is currently composing syllables (e.g. Korean Hangul), commit preedit only without submitting modal
                if this.marked_range.is_some() {
                    cx.stop_propagation();
                    return;
                }
                this.marked_range = None;
                cx.emit(TextInputEvent::Submit);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &Cancel, _window, cx| {
                // If IME is composing, Escape cancels the active preedit block without closing the parent modal
                if this.marked_range.is_some() {
                    this.marked_range = None;
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                this.marked_range = None;
                cx.emit(TextInputEvent::Cancel);
                cx.stop_propagation();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextElement { input: cx.entity() })
    }
}

/// Finds the UTF-8 word boundary range for word selection at a byte offset using Unicode Annex #29.
/// Guaranteed to align to valid char boundaries on multi-byte UTF-8 sequences (Korean, CJK, Emoji).
pub fn find_word_range_at(content: &str, offset: usize) -> Range<usize> {
    if content.is_empty() {
        return 0..0;
    }
    let offset = offset.min(content.len());
    let mut target_range = 0..content.len();

    let segments: Vec<(usize, &str)> = content.split_word_bound_indices().collect();
    for (i, &(start, word)) in segments.iter().enumerate() {
        let end = start + word.len();
        if offset >= start && offset <= end {
            let is_whitespace = word.chars().all(|c| c.is_whitespace());
            if is_whitespace {
                // If offset lands exactly on the boundary between word and whitespace, prefer selecting the word
                if offset == start && i > 0 {
                    let (prev_start, prev_word) = segments[i - 1];
                    if !prev_word.chars().all(|c| c.is_whitespace()) {
                        target_range = prev_start..start;
                        break;
                    }
                } else if offset == end && i + 1 < segments.len() {
                    let (next_start, next_word) = segments[i + 1];
                    if !next_word.chars().all(|c| c.is_whitespace()) {
                        target_range = next_start..next_start + next_word.len();
                        break;
                    }
                }
            }
            target_range = start..end;
            if offset < end || is_whitespace {
                break;
            }
        }
    }
    target_range
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_input_events_variants() {
        let submit = TextInputEvent::Submit;
        let cancel = TextInputEvent::Cancel;
        let change = TextInputEvent::Change;
        assert_ne!(submit, cancel);
        assert_ne!(submit, change);
        assert_eq!(submit, TextInputEvent::Submit);
        assert_eq!(cancel, TextInputEvent::Cancel);
        assert_eq!(change, TextInputEvent::Change);
    }

    #[test]
    fn test_ime_composition_marked_range_semantics() {
        // Test that marked_range is tracked accurately during IME preedit
        let mut marked_range = Some(0..6);
        assert!(marked_range.is_some(), "Preedit must be active");

        // Simulating Submit action when IME is active: must not emit submit
        let should_submit = marked_range.is_none();
        assert!(!should_submit, "Must guard submit during active IME composition");

        // Simulating Cancel action when IME is active: clears preedit without closing modal
        if marked_range.is_some() {
            marked_range = None;
        }
        assert!(marked_range.is_none());

        // Subsequent Submit after IME committed
        let should_submit_now = marked_range.is_none();
        assert!(should_submit_now, "Can submit after IME commit");
    }

    #[test]
    fn test_find_word_range_korean_cjk_emoji_no_panic() {
        let content = "안녕하세요 로그 분석";
        // Korean characters are 3 bytes each.
        // "안녕하세요" = 15 bytes (0..15)
        // " " = 1 byte (15..16)
        // "로그" = 6 bytes (16..22)
        // " " = 1 byte (22..23)
        // "분석" = 6 bytes (23..29)

        // Test offset in middle of first word
        let r1 = find_word_range_at(content, 6);
        assert_eq!(r1, 0..15);
        assert_eq!(&content[r1], "안녕하세요");

        // Test offset at boundary between first word and space
        let r_bound = find_word_range_at(content, 15);
        assert_eq!(r_bound, 0..15);
        assert_eq!(&content[r_bound], "안녕하세요");

        // Test offset in middle of second word
        let r2 = find_word_range_at(content, 19);
        assert_eq!(r2, 16..22);
        assert_eq!(&content[r2], "로그");

        // Test offset in middle of third word
        let r3 = find_word_range_at(content, 26);
        assert_eq!(r3, 23..29);
        assert_eq!(&content[r3], "분석");

        // Fuzz every single byte offset: every result must be a valid char boundary without panic!
        for offset in 0..=content.len() {
            let r = find_word_range_at(content, offset);
            assert!(content.is_char_boundary(r.start));
            assert!(content.is_char_boundary(r.end));
            assert!(r.start <= r.end);
            let _slice = &content[r];
        }

        // Test Japanese and Emoji
        let jp_emoji = "こんにちは 世界 🚀🦀";
        for offset in 0..=jp_emoji.len() {
            let r = find_word_range_at(jp_emoji, offset);
            assert!(jp_emoji.is_char_boundary(r.start));
            assert!(jp_emoji.is_char_boundary(r.end));
            let _slice = &jp_emoji[r];
        }
    }

    #[test]
    fn test_find_word_range_ascii_and_whitespace() {
        let text = "hello   world";
        let r_hello = find_word_range_at(text, 2);
        assert_eq!(&text[r_hello], "hello");

        // Inside whitespace
        let r_spaces = find_word_range_at(text, 6);
        assert_eq!(&text[r_spaces], "   ");

        let r_world = find_word_range_at(text, 10);
        assert_eq!(&text[r_world], "world");

        // Empty string
        assert_eq!(find_word_range_at("", 0), 0..0);
    }

    #[test]
    fn test_utf16_range_to_utf8_in_str_mixed_ascii_and_cjk() {
        let text = "Hello 한국어 🚀 World";
        // "Hello " = 6 bytes, 6 UTF-16 code units
        // "한국어" = 9 bytes (3 UTF-8 chars, 3 bytes each), 3 UTF-16 code units
        // " " = 1 byte, 1 UTF-16 code unit
        // "🚀" = 4 bytes (1 UTF-8 char, 4 bytes), 2 UTF-16 code units (surrogate pair)
        // " World" = 6 bytes, 6 UTF-16 code units

        // UTF-16 range for "한국어": start at 6, end at 9 (len 3)
        let r_korean = utf16_range_to_utf8_in_str(text, &(6..9));
        assert_eq!(r_korean, 6..15);
        assert_eq!(&text[r_korean], "한국어");

        // UTF-16 range for "🚀": start at 10, end at 12 (len 2 UTF-16 code units)
        let r_rocket = utf16_range_to_utf8_in_str(text, &(10..12));
        assert_eq!(r_rocket, 16..20);
        assert_eq!(&text[r_rocket], "🚀");

        // Test every possible UTF-16 slice: must always align to valid UTF-8 char boundaries
        let total_utf16 = text.encode_utf16().count();
        for start_u16 in 0..=total_utf16 {
            for end_u16 in start_u16..=total_utf16 {
                let r_utf8 = utf16_range_to_utf8_in_str(text, &(start_u16..end_u16));
                assert!(text.is_char_boundary(r_utf8.start));
                assert!(text.is_char_boundary(r_utf8.end));
                assert!(r_utf8.start <= r_utf8.end);
            }
        }
    }
}
