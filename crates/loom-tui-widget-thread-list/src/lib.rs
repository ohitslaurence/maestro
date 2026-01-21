// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Modifier, Style},
	text::{Line, Span},
	widgets::StatefulWidget,
};
use unicode_width::UnicodeWidthStr;

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
	let width = UnicodeWidthStr::width(s);
	if width <= max_width {
		return s.to_string();
	}
	if max_width == 0 {
		return String::new();
	}
	if max_width == 1 {
		return "…".to_string();
	}

	let mut result = String::new();
	let mut current_width = 0;
	let target_width = max_width.saturating_sub(1);

	for c in s.chars() {
		let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
		if current_width + char_width > target_width {
			break;
		}
		result.push(c);
		current_width += char_width;
	}
	result.push('…');
	result
}

#[derive(Debug, Clone)]
pub struct ThreadItem {
	pub id: String,
	pub title: String,
	pub preview: String,
	pub timestamp: String,
	pub unread: bool,
}

impl ThreadItem {
	pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
		Self {
			id: id.into(),
			title: title.into(),
			preview: String::new(),
			timestamp: String::new(),
			unread: false,
		}
	}

	pub fn preview(mut self, preview: impl Into<String>) -> Self {
		self.preview = preview.into();
		self
	}

	pub fn timestamp(mut self, timestamp: impl Into<String>) -> Self {
		self.timestamp = timestamp.into();
		self
	}

	pub fn unread(mut self, unread: bool) -> Self {
		self.unread = unread;
		self
	}
}

#[derive(Debug, Default, Clone)]
pub struct ThreadListState {
	selected: usize,
	scroll_offset: usize,
}

impl ThreadListState {
	pub fn clamp_to_total(&mut self, total: usize) {
		if total == 0 {
			self.selected = 0;
			self.scroll_offset = 0;
		} else {
			self.selected = self.selected.min(total - 1);
			self.scroll_offset = self.scroll_offset.min(total.saturating_sub(1));
		}
	}

	pub fn select_next(&mut self, total: usize) {
		if total > 0 {
			self.selected = (self.selected + 1).min(total - 1);
		}
	}

	pub fn select_prev(&mut self) {
		self.selected = self.selected.saturating_sub(1);
	}

	pub fn page_down(&mut self, visible_items: usize, total: usize) {
		if total > 0 {
			self.selected = (self.selected + visible_items).min(total - 1);
		}
	}

	pub fn page_up(&mut self, visible_items: usize) {
		self.selected = self.selected.saturating_sub(visible_items);
	}

	pub fn selected(&self) -> Option<usize> {
		Some(self.selected)
	}

	pub fn selected_index(&self) -> usize {
		self.selected
	}

	pub fn scroll_offset(&self) -> usize {
		self.scroll_offset
	}

	pub fn set_selected(&mut self, selected: usize) {
		self.selected = selected;
	}
}

#[derive(Debug, Clone)]
pub struct ThreadList {
	threads: Vec<ThreadItem>,
	style: Style,
	focused: bool,
	direction: TextDirection,
}

impl ThreadList {
	pub fn new(threads: Vec<ThreadItem>) -> Self {
		Self {
			threads,
			style: Style::default(),
			focused: false,
			direction: TextDirection::default(),
		}
	}

	pub fn style(mut self, style: Style) -> Self {
		self.style = style;
		self
	}

	pub fn focused(mut self, focused: bool) -> Self {
		self.focused = focused;
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}
}

impl StatefulWidget for ThreadList {
	type State = ThreadListState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		if area.height == 0 || area.width == 0 {
			return;
		}

		state.clamp_to_total(self.threads.len());

		let item_height = 3;
		let visible_items = (area.height as usize) / item_height;

		if state.selected >= state.scroll_offset + visible_items {
			state.scroll_offset = state.selected.saturating_sub(visible_items - 1);
		} else if state.selected < state.scroll_offset {
			state.scroll_offset = state.selected;
		}

		let mut y = area.y;
		let max_y = area.y + area.height;

		for (idx, thread) in self.threads.iter().enumerate().skip(state.scroll_offset) {
			if y + 2 > max_y {
				break;
			}

			let is_selected = idx == state.selected;

			let line_style = if is_selected {
				self.style.add_modifier(Modifier::REVERSED)
			} else {
				self.style
			};

			let is_rtl = self.direction.is_rtl();
			let unread_indicator = if thread.unread {
				if is_rtl { " ●" } else { "● " }
			} else {
				"  "
			};
			let title_style = if thread.unread {
				line_style.add_modifier(Modifier::BOLD)
			} else {
				line_style
			};

			let title_width = area.width.saturating_sub(2) as usize;
			let timestamp_width = UnicodeWidthStr::width(thread.timestamp.as_str());
			let unread_width = UnicodeWidthStr::width(unread_indicator);
			let available_title = title_width.saturating_sub(timestamp_width + 1 + unread_width);
			let truncated_title = truncate_with_ellipsis(&thread.title, available_title);

			let truncated_title_width = UnicodeWidthStr::width(truncated_title.as_str());
			let padding = title_width.saturating_sub(unread_width + truncated_title_width + timestamp_width);
			let title_line = if is_rtl {
				Line::from(vec![
					Span::styled(&thread.timestamp, line_style),
					Span::styled(" ".repeat(padding), line_style),
					Span::styled(&truncated_title, title_style),
					Span::styled(unread_indicator, title_style),
				])
			} else {
				Line::from(vec![
					Span::styled(unread_indicator, title_style),
					Span::styled(&truncated_title, title_style),
					Span::styled(" ".repeat(padding), line_style),
					Span::styled(&thread.timestamp, line_style),
				])
			};
			buf.set_line(area.x, y, &title_line, area.width);
			y += 1;

			if y >= max_y {
				break;
			}

			let preview_width = area.width.saturating_sub(4) as usize;
			let truncated_preview = truncate_with_ellipsis(&thread.preview, preview_width);

			let preview_style = if is_selected {
				line_style
			} else {
				self.style.add_modifier(Modifier::DIM)
			};
			let preview_line = Line::from(vec![
				Span::styled("    ", preview_style),
				Span::styled(&truncated_preview, preview_style),
			]);
			buf.set_line(area.x, y, &preview_line, area.width);
			y += 1;

			if y < max_y {
				y += 1;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_thread_item_builder() {
		let thread = ThreadItem::new("1", "Test Thread")
			.preview("This is a preview")
			.timestamp("12:00")
			.unread(true);

		assert_eq!(thread.id, "1");
		assert_eq!(thread.title, "Test Thread");
		assert_eq!(thread.preview, "This is a preview");
		assert_eq!(thread.timestamp, "12:00");
		assert!(thread.unread);
	}

	#[test]
	fn test_thread_list_state_navigation() {
		let mut state = ThreadListState::default();
		assert_eq!(state.selected, 0);

		state.select_next(5);
		assert_eq!(state.selected, 1);

		state.select_next(5);
		state.select_next(5);
		state.select_next(5);
		assert_eq!(state.selected, 4);

		state.select_next(5);
		assert_eq!(state.selected, 4);

		state.select_prev();
		assert_eq!(state.selected, 3);

		state.select_prev();
		state.select_prev();
		state.select_prev();
		assert_eq!(state.selected, 0);

		state.select_prev();
		assert_eq!(state.selected, 0);
	}

	#[test]
	fn test_thread_list_state_empty() {
		let mut state = ThreadListState::default();
		state.select_next(0);
		assert_eq!(state.selected, 0);
	}

	#[test]
	fn test_clamp_to_total() {
		let mut state = ThreadListState::default();
		state.selected = 10;
		state.scroll_offset = 5;

		state.clamp_to_total(5);
		assert_eq!(state.selected, 4);
		assert_eq!(state.scroll_offset, 4);

		state.clamp_to_total(0);
		assert_eq!(state.selected, 0);
		assert_eq!(state.scroll_offset, 0);
	}

	#[test]
	fn test_page_navigation() {
		let mut state = ThreadListState::default();

		state.page_down(5, 20);
		assert_eq!(state.selected, 5);

		state.page_down(5, 20);
		assert_eq!(state.selected, 10);

		state.page_down(5, 12);
		assert_eq!(state.selected, 11);

		state.page_up(5);
		assert_eq!(state.selected, 6);

		state.page_up(10);
		assert_eq!(state.selected, 0);
	}

	#[test]
	fn test_truncate_with_ellipsis() {
		assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
		assert_eq!(truncate_with_ellipsis("hello world", 8), "hello w…");
		assert_eq!(truncate_with_ellipsis("hello", 0), "");
		assert_eq!(truncate_with_ellipsis("hello", 1), "…");
		assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
	}
}
