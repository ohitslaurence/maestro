// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use crossterm::event::{KeyCode, KeyEvent};
use loom_tui_widget_message_list::{Message, MessageList, MessageListState, MessageRole};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct EmptyMessageList {
	state: MessageListState,
}

impl StoryComponent for EmptyMessageList {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let list = MessageList::new(vec![]);
		list.render(area, frame.buffer_mut(), &mut self.state);
	}
}

struct MessageListWithMessages {
	messages: Vec<Message>,
	state: MessageListState,
}

impl MessageListWithMessages {
	fn new() -> Self {
		let messages = vec![
			Message::new(MessageRole::User, "Hello, can you help me with Rust?"),
			Message::new(
				MessageRole::Assistant,
				"Of course! I'd be happy to help you with Rust. What would you like to know?",
			),
			Message::new(MessageRole::User, "How do I handle errors?")
				.with_timestamp("12:34"),
			Message::new(
				MessageRole::Assistant,
				"Rust has a powerful error handling system using Result<T, E> and the ? operator.",
			),
		];
		Self {
			messages,
			state: MessageListState::default(),
		}
	}
}

impl StoryComponent for MessageListWithMessages {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let list = MessageList::new(self.messages.clone());
		list.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Up => self.state.scroll_up(1),
			KeyCode::Down => self.state.scroll_down(1, self.messages.len()),
			_ => {}
		}
	}
}

pub fn message_list_story() -> Story {
	Story::new("MessageList", "List of chat messages")
		.variant("Empty", EmptyMessageList { state: MessageListState::default() })
		.variant("With Messages", MessageListWithMessages::new())
}
