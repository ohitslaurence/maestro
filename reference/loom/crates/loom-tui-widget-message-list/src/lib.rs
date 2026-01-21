// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Modifier, Style},
	text::{Line, Span, Text},
	widgets::StatefulWidget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
	User,
	Assistant,
	System,
}

impl MessageRole {
	pub fn label(&self) -> &'static str {
		match self {
			MessageRole::User => "You",
			MessageRole::Assistant => "Loom",
			MessageRole::System => "System",
		}
	}
}

#[derive(Debug, Clone)]
pub enum MessageContent {
	Plain(String),
	Rich(Text<'static>),
}

#[derive(Debug, Clone)]
pub struct Message {
	pub role: MessageRole,
	pub content: MessageContent,
	pub timestamp: Option<String>,
}

impl Message {
	pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
		Self {
			role,
			content: MessageContent::Plain(content.into()),
			timestamp: None,
		}
	}

	pub fn new_rich(role: MessageRole, content: Text<'static>) -> Self {
		Self {
			role,
			content: MessageContent::Rich(content),
			timestamp: None,
		}
	}

	pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
		self.timestamp = Some(timestamp.into());
		self
	}
}

#[derive(Debug, Default, Clone)]
pub struct MessageListState {
	pub scroll_offset: usize,
	pub selected: Option<usize>,
}

impl MessageListState {
	pub fn scroll_up(&mut self, amount: usize) {
		self.scroll_offset = self.scroll_offset.saturating_sub(amount);
	}

	pub fn scroll_down(&mut self, amount: usize, total_messages: usize) {
		if total_messages == 0 {
			self.scroll_offset = 0;
			return;
		}
		let max = total_messages.saturating_sub(1);
		self.scroll_offset = (self.scroll_offset + amount).min(max);
	}

	pub fn scroll_to_bottom(&mut self, total_messages: usize) {
		if total_messages == 0 {
			self.scroll_offset = 0;
		} else {
			self.scroll_offset = total_messages.saturating_sub(1);
		}
	}

	pub fn select_next(&mut self, total_messages: usize) {
		if total_messages == 0 {
			self.selected = None;
			return;
		}
		self.selected = Some(match self.selected {
			Some(idx) => (idx + 1).min(total_messages.saturating_sub(1)),
			None => 0,
		});
	}

	pub fn select_prev(&mut self, total_messages: usize) {
		if total_messages == 0 {
			self.selected = None;
			return;
		}
		self.selected = Some(match self.selected {
			Some(idx) => idx.saturating_sub(1),
			None => 0,
		});
	}
}

#[derive(Debug, Clone)]
pub struct MessageList {
	messages: Vec<Message>,
	style: Style,
	header_style: Style,
	timestamp_style: Style,
	selected_style: Style,
	direction: TextDirection,
}

impl MessageList {
	pub fn new(messages: Vec<Message>) -> Self {
		Self {
			messages,
			style: Style::default(),
			header_style: Style::default(),
			timestamp_style: Style::default().add_modifier(Modifier::DIM),
			selected_style: Style::default(),
			direction: TextDirection::default(),
		}
	}

	pub fn style(mut self, style: Style) -> Self {
		self.style = style;
		self
	}

	pub fn header_style(mut self, style: Style) -> Self {
		self.header_style = style;
		self
	}

	pub fn timestamp_style(mut self, style: Style) -> Self {
		self.timestamp_style = style;
		self
	}

	pub fn selected_style(mut self, style: Style) -> Self {
		self.selected_style = style;
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}
}

impl StatefulWidget for MessageList {
	type State = MessageListState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		if area.height == 0 || area.width == 0 {
			return;
		}

		let total_messages = self.messages.len();
		if total_messages > 0 {
			let max = total_messages.saturating_sub(1);
			state.scroll_offset = state.scroll_offset.min(max);
		} else {
			state.scroll_offset = 0;
		}

		let is_rtl = self.direction.is_rtl();
		let mut y = area.y;
		let max_y = area.y + area.height;
		let mut prev_role: Option<MessageRole> = None;

		for (idx, message) in self.messages.iter().enumerate().skip(state.scroll_offset) {
			if y >= max_y {
				break;
			}

			let is_selected = state.selected == Some(idx);
			let base_style = if is_selected { self.selected_style } else { self.style };

			let same_as_prev = prev_role == Some(message.role);
			prev_role = Some(message.role);

			if !same_as_prev {
				let role_label = message.role.label();

				let header_line = if is_rtl {
					let role_span = Span::styled(format!(" :{}", role_label), self.header_style);
					if let Some(ts) = &message.timestamp {
						let ts_span = Span::styled(format!("[{}]", ts), self.timestamp_style);
						let ts_len = ts.len() + 2;
						let role_len = role_label.len() + 2;
						let total_len = ts_len + role_len;
						let padding = (area.width as usize).saturating_sub(total_len);
						Line::from(vec![ts_span, Span::raw(" ".repeat(padding)), role_span])
					} else {
						let padding = if (area.width as usize) > role_label.len() + 2 {
							area.width as usize - role_label.len() - 2
						} else {
							0
						};
						Line::from(vec![Span::raw(" ".repeat(padding)), role_span])
					}
				} else {
					let header_span = Span::styled(format!("{}: ", role_label), self.header_style);
					if let Some(ts) = &message.timestamp {
						let header_len = role_label.len() + 2;
						let ts_len = ts.len();
						let total_len = header_len + ts_len;
						let padding = (area.width as usize).saturating_sub(total_len);
						Line::from(vec![
							header_span,
							Span::raw(" ".repeat(padding)),
							Span::styled(ts.clone(), self.timestamp_style),
						])
					} else {
						Line::from(vec![header_span])
					}
				};
				buf.set_line(area.x, y, &header_line, area.width);
				y += 1;

				if y >= max_y {
					break;
				}
			}

			match &message.content {
				MessageContent::Plain(text) => {
					for line in text.lines() {
						if y >= max_y {
							break;
						}

						let content_line = if is_rtl {
							let line_len = line.len() + 2;
							let padding = (area.width as usize).saturating_sub(line_len);
							Line::from(vec![
								Span::raw(" ".repeat(padding)),
								Span::styled(line.to_string(), base_style),
								Span::raw("  "),
							])
						} else {
							let indent = "  ";
							Line::from(vec![Span::styled(format!("{}{}", indent, line), base_style)])
						};
						buf.set_line(area.x, y, &content_line, area.width);
						y += 1;
					}
				}
				MessageContent::Rich(text) => {
					for line in text.lines.iter() {
						if y >= max_y {
							break;
						}

						let content_line = if is_rtl {
							let mut spans: Vec<Span> = Vec::new();
							let mut content_width: usize = 0;
							for span in line.spans.iter() {
								content_width += span.content.len();
								let mut styled_span = span.clone();
								styled_span.style = styled_span.style.patch(base_style);
								spans.push(styled_span);
							}
							content_width += 2;
							let padding = (area.width as usize).saturating_sub(content_width);
							let mut result = vec![Span::raw(" ".repeat(padding))];
							result.extend(spans);
							result.push(Span::raw("  "));
							Line::from(result)
						} else {
							let mut spans = vec![Span::raw("  ")];
							for span in line.spans.iter() {
								let mut styled_span = span.clone();
								styled_span.style = styled_span.style.patch(base_style);
								spans.push(styled_span);
							}
							Line::from(spans)
						};
						buf.set_line(area.x, y, &content_line, area.width);
						y += 1;
					}
				}
			}

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
	fn test_message_role_labels() {
		assert_eq!(MessageRole::User.label(), "You");
		assert_eq!(MessageRole::Assistant.label(), "Loom");
		assert_eq!(MessageRole::System.label(), "System");
	}

	#[test]
	fn test_message_list_state_scroll() {
		let mut state = MessageListState::default();
		assert_eq!(state.scroll_offset, 0);

		state.scroll_down(5, 10);
		assert_eq!(state.scroll_offset, 5);

		state.scroll_down(10, 10);
		assert_eq!(state.scroll_offset, 9);

		state.scroll_up(3);
		assert_eq!(state.scroll_offset, 6);

		state.scroll_up(100);
		assert_eq!(state.scroll_offset, 0);
	}

	#[test]
	fn test_scroll_down_empty() {
		let mut state = MessageListState::default();
		state.scroll_down(5, 0);
		assert_eq!(state.scroll_offset, 0);
	}

	#[test]
	fn test_scroll_to_bottom() {
		let mut state = MessageListState::default();
		state.scroll_to_bottom(10);
		assert_eq!(state.scroll_offset, 9);

		state.scroll_to_bottom(0);
		assert_eq!(state.scroll_offset, 0);
	}

	#[test]
	fn test_select_next_prev() {
		let mut state = MessageListState::default();
		assert_eq!(state.selected, None);

		state.select_next(5);
		assert_eq!(state.selected, Some(0));

		state.select_next(5);
		assert_eq!(state.selected, Some(1));

		state.select_next(5);
		state.select_next(5);
		state.select_next(5);
		state.select_next(5);
		assert_eq!(state.selected, Some(4));

		state.select_prev(5);
		assert_eq!(state.selected, Some(3));

		state.select_prev(5);
		state.select_prev(5);
		state.select_prev(5);
		state.select_prev(5);
		assert_eq!(state.selected, Some(0));
	}

	#[test]
	fn test_select_empty() {
		let mut state = MessageListState::default();
		state.select_next(0);
		assert_eq!(state.selected, None);

		state.select_prev(0);
		assert_eq!(state.selected, None);
	}

	#[test]
	fn test_message_builder() {
		let msg = Message::new(MessageRole::User, "Hello").with_timestamp("12:00");

		assert_eq!(msg.role, MessageRole::User);
		assert!(matches!(msg.content, MessageContent::Plain(ref s) if s == "Hello"));
		assert_eq!(msg.timestamp, Some("12:00".to_string()));
	}

	#[test]
	fn test_message_rich() {
		let text = Text::raw("Rich content");
		let msg = Message::new_rich(MessageRole::Assistant, text);

		assert_eq!(msg.role, MessageRole::Assistant);
		assert!(matches!(msg.content, MessageContent::Rich(_)));
	}
}
