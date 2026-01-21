// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::Style,
	widgets::StatefulWidget,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Default, Clone)]
pub struct InputBoxState {
	content: String,
	cursor_position: usize,
	scroll_offset: usize,
}

impl InputBoxState {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn insert_char(&mut self, c: char) {
		self.content.insert(self.cursor_position, c);
		self.cursor_position += c.len_utf8();
	}

	pub fn delete_char(&mut self) {
		if self.cursor_position > 0 {
			let prev_grapheme_start = self.content[..self.cursor_position]
				.grapheme_indices(true)
				.next_back()
				.map(|(i, _)| i)
				.unwrap_or(0);
			self.content.drain(prev_grapheme_start..self.cursor_position);
			self.cursor_position = prev_grapheme_start;
		}
	}

	pub fn delete_char_forward(&mut self) {
		if self.cursor_position < self.content.len() {
			if let Some((_, grapheme)) = self.content[self.cursor_position..].grapheme_indices(true).next() {
				let grapheme_len = grapheme.len();
				self.content.drain(self.cursor_position..self.cursor_position + grapheme_len);
			}
		}
	}

	pub fn move_cursor_left(&mut self) {
		if self.cursor_position > 0 {
			self.cursor_position = self.content[..self.cursor_position]
				.grapheme_indices(true)
				.next_back()
				.map(|(i, _)| i)
				.unwrap_or(0);
		}
	}

	pub fn move_cursor_right(&mut self) {
		if self.cursor_position < self.content.len() {
			if let Some((_, grapheme)) = self.content[self.cursor_position..].grapheme_indices(true).next() {
				self.cursor_position += grapheme.len();
			}
		}
	}

	pub fn move_cursor_start(&mut self) {
		self.cursor_position = 0;
	}

	pub fn move_cursor_end(&mut self) {
		self.cursor_position = self.content.len();
	}

	pub fn move_cursor_prev_word(&mut self) {
		if self.cursor_position == 0 {
			return;
		}
		let before_cursor = &self.content[..self.cursor_position];
		let mut new_pos = 0;
		let mut found_word = false;
		for (i, grapheme) in before_cursor.grapheme_indices(true).rev() {
			let is_whitespace = grapheme.chars().all(|c| c.is_whitespace());
			if !found_word {
				if !is_whitespace {
					found_word = true;
				}
			} else if is_whitespace {
				new_pos = i + grapheme.len();
				break;
			}
		}
		self.cursor_position = new_pos;
	}

	pub fn move_cursor_next_word(&mut self) {
		if self.cursor_position >= self.content.len() {
			return;
		}
		let after_cursor = &self.content[self.cursor_position..];
		let mut offset = 0;
		let mut found_whitespace = false;
		for (i, grapheme) in after_cursor.grapheme_indices(true) {
			let is_whitespace = grapheme.chars().all(|c| c.is_whitespace());
			if !found_whitespace {
				if is_whitespace {
					found_whitespace = true;
				}
			} else if !is_whitespace {
				offset = i;
				break;
			}
			offset = i + grapheme.len();
		}
		self.cursor_position += offset;
	}

	pub fn delete_prev_word(&mut self) {
		if self.cursor_position == 0 {
			return;
		}
		let before_cursor = &self.content[..self.cursor_position];
		let mut delete_start = 0;
		let mut found_word = false;
		for (i, grapheme) in before_cursor.grapheme_indices(true).rev() {
			let is_whitespace = grapheme.chars().all(|c| c.is_whitespace());
			if !found_word {
				if !is_whitespace {
					found_word = true;
				}
			} else if is_whitespace {
				delete_start = i + grapheme.len();
				break;
			}
		}
		self.content.drain(delete_start..self.cursor_position);
		self.cursor_position = delete_start;
	}

	pub fn delete_next_word(&mut self) {
		if self.cursor_position >= self.content.len() {
			return;
		}
		let after_cursor = &self.content[self.cursor_position..];
		let mut delete_end_offset = 0;
		let mut found_whitespace = false;
		for (i, grapheme) in after_cursor.grapheme_indices(true) {
			let is_whitespace = grapheme.chars().all(|c| c.is_whitespace());
			if !found_whitespace {
				if is_whitespace {
					found_whitespace = true;
				}
			} else if !is_whitespace {
				delete_end_offset = i;
				break;
			}
			delete_end_offset = i + grapheme.len();
		}
		self.content.drain(self.cursor_position..self.cursor_position + delete_end_offset);
	}

	pub fn content(&self) -> &str {
		&self.content
	}

	pub fn clear(&mut self) {
		self.content.clear();
		self.cursor_position = 0;
		self.scroll_offset = 0;
	}

	pub fn cursor_position(&self) -> usize {
		self.cursor_position
	}

	pub fn scroll_offset(&self) -> usize {
		self.scroll_offset
	}
}

#[derive(Debug, Clone)]
pub struct InputBox {
	placeholder: Option<String>,
	style: Style,
	cursor_style: Style,
	focused: bool,
	direction: TextDirection,
}

impl Default for InputBox {
	fn default() -> Self {
		Self::new()
	}
}

impl InputBox {
	pub fn new() -> Self {
		Self {
			placeholder: None,
			style: Style::default(),
			cursor_style: Style::default().bg(ratatui::style::Color::White).fg(ratatui::style::Color::Black),
			focused: false,
			direction: TextDirection::Ltr,
		}
	}

	pub fn placeholder(mut self, text: impl Into<String>) -> Self {
		self.placeholder = Some(text.into());
		self
	}

	pub fn style(mut self, style: Style) -> Self {
		self.style = style;
		self
	}

	pub fn cursor_style(mut self, style: Style) -> Self {
		self.cursor_style = style;
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

impl StatefulWidget for InputBox {
	type State = InputBoxState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		if area.width == 0 || area.height == 0 {
			return;
		}

		let width = area.width as usize;
		let is_rtl = self.direction.is_rtl();

		if state.content.is_empty() {
			if let Some(ref placeholder) = self.placeholder {
				let placeholder_style = self.style.fg(ratatui::style::Color::DarkGray);
				let display: String = placeholder.graphemes(true).take(width).collect();
				let display_width = display.graphemes(true).count() as u16;
				let placeholder_x = if is_rtl {
					area.x + area.width.saturating_sub(display_width)
				} else {
					area.x
				};
				buf.set_string(placeholder_x, area.y, &display, placeholder_style);
			}
			let cursor_x = if is_rtl {
				area.x + area.width.saturating_sub(1)
			} else {
				area.x
			};
			buf.set_string(cursor_x, area.y, " ", self.cursor_style);
			return;
		}

		let cursor_grapheme_pos = state.content[..state.cursor_position].graphemes(true).count();

		if cursor_grapheme_pos < state.scroll_offset {
			state.scroll_offset = cursor_grapheme_pos;
		} else if cursor_grapheme_pos >= state.scroll_offset + width {
			state.scroll_offset = cursor_grapheme_pos.saturating_sub(width - 1);
		}

		let visible_graphemes: Vec<&str> = state.content.graphemes(true).skip(state.scroll_offset).take(width).collect();
		let visible_width = visible_graphemes.len() as u16;
		let cursor_display_pos = cursor_grapheme_pos - state.scroll_offset;

		let text_start_x = if is_rtl {
			area.x + area.width.saturating_sub(visible_width)
		} else {
			area.x
		};

		for (i, g) in visible_graphemes.iter().enumerate() {
			let x = if is_rtl {
				text_start_x + visible_width.saturating_sub(1).saturating_sub(i as u16)
			} else {
				text_start_x + i as u16
			};
			let style = if i == cursor_display_pos {
				self.cursor_style
			} else {
				self.style
			};
			buf.set_string(x, area.y, *g, style);
		}

		if cursor_display_pos >= visible_graphemes.len() && cursor_display_pos < width {
			let cursor_x = if is_rtl {
				text_start_x.saturating_sub(1)
			} else {
				text_start_x + cursor_display_pos as u16
			};
			buf.set_string(cursor_x, area.y, " ", self.cursor_style);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_insert_and_cursor() {
		let mut state = InputBoxState::new();
		state.insert_char('h');
		state.insert_char('i');
		assert_eq!(state.content(), "hi");
		assert_eq!(state.cursor_position(), 2);
	}

	#[test]
	fn test_delete_char() {
		let mut state = InputBoxState::new();
		state.insert_char('a');
		state.insert_char('b');
		state.delete_char();
		assert_eq!(state.content(), "a");
		assert_eq!(state.cursor_position(), 1);
	}

	#[test]
	fn test_cursor_movement() {
		let mut state = InputBoxState::new();
		state.insert_char('a');
		state.insert_char('b');
		state.insert_char('c');
		state.move_cursor_start();
		assert_eq!(state.cursor_position(), 0);
		state.move_cursor_end();
		assert_eq!(state.cursor_position(), 3);
		state.move_cursor_left();
		assert_eq!(state.cursor_position(), 2);
		state.move_cursor_right();
		assert_eq!(state.cursor_position(), 3);
	}

	#[test]
	fn test_clear() {
		let mut state = InputBoxState::new();
		state.insert_char('x');
		state.clear();
		assert_eq!(state.content(), "");
		assert_eq!(state.cursor_position(), 0);
	}

	#[test]
	fn test_grapheme_navigation() {
		let mut state = InputBoxState::new();
		for c in "héllo".chars() {
			state.insert_char(c);
		}
		state.move_cursor_start();
		state.move_cursor_right();
		state.move_cursor_right();
		assert_eq!(state.cursor_position(), 3);
		state.move_cursor_left();
		assert_eq!(state.cursor_position(), 1);
	}

	#[test]
	fn test_word_navigation() {
		let mut state = InputBoxState::new();
		for c in "hello world test".chars() {
			state.insert_char(c);
		}
		state.move_cursor_start();
		state.move_cursor_next_word();
		assert_eq!(state.cursor_position(), 6);
		state.move_cursor_next_word();
		assert_eq!(state.cursor_position(), 12);
		state.move_cursor_prev_word();
		assert_eq!(state.cursor_position(), 6);
		state.move_cursor_prev_word();
		assert_eq!(state.cursor_position(), 0);
	}

	#[test]
	fn test_delete_prev_word() {
		let mut state = InputBoxState::new();
		for c in "hello world".chars() {
			state.insert_char(c);
		}
		state.delete_prev_word();
		assert_eq!(state.content(), "hello ");
	}

	#[test]
	fn test_delete_next_word() {
		let mut state = InputBoxState::new();
		for c in "hello world test".chars() {
			state.insert_char(c);
		}
		state.move_cursor_start();
		state.delete_next_word();
		assert_eq!(state.content(), "world test");
	}
}
