// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use crossterm::event::{KeyCode, KeyEvent};
use loom_tui_widget_input_box::{InputBox, InputBoxState};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct EmptyInputBox {
	state: InputBoxState,
}

impl StoryComponent for EmptyInputBox {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let input = InputBox::new();
		input.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Char(c) => self.state.insert_char(c),
			KeyCode::Backspace => { self.state.delete_char(); }
			KeyCode::Left => self.state.move_cursor_left(),
			KeyCode::Right => self.state.move_cursor_right(),
			KeyCode::Home => self.state.move_cursor_start(),
			KeyCode::End => self.state.move_cursor_end(),
			_ => {}
		}
	}
}

struct InputBoxWithPlaceholder {
	state: InputBoxState,
}

impl StoryComponent for InputBoxWithPlaceholder {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let input = InputBox::new().placeholder("Type your message...");
		input.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Char(c) => self.state.insert_char(c),
			KeyCode::Backspace => { self.state.delete_char(); }
			KeyCode::Left => self.state.move_cursor_left(),
			KeyCode::Right => self.state.move_cursor_right(),
			KeyCode::Home => self.state.move_cursor_start(),
			KeyCode::End => self.state.move_cursor_end(),
			_ => {}
		}
	}
}

struct InputBoxWithContent {
	state: InputBoxState,
}

impl InputBoxWithContent {
	fn new() -> Self {
		let mut state = InputBoxState::new();
		for c in "Hello World".chars() {
			state.insert_char(c);
		}
		Self { state }
	}
}

impl StoryComponent for InputBoxWithContent {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let input = InputBox::new();
		input.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Char(c) => self.state.insert_char(c),
			KeyCode::Backspace => { self.state.delete_char(); }
			KeyCode::Left => self.state.move_cursor_left(),
			KeyCode::Right => self.state.move_cursor_right(),
			KeyCode::Home => self.state.move_cursor_start(),
			KeyCode::End => self.state.move_cursor_end(),
			_ => {}
		}
	}
}

pub fn input_box_story() -> Story {
	Story::new("InputBox", "Text input field with cursor")
		.variant("Empty", EmptyInputBox { state: InputBoxState::new() })
		.variant("With Placeholder", InputBoxWithPlaceholder { state: InputBoxState::new() })
		.variant("With Content", InputBoxWithContent::new())
}
