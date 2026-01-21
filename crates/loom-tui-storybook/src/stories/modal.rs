// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use crossterm::event::{KeyCode, KeyEvent};
use loom_tui_widget_modal::{Modal, ModalState};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct ConfirmDialog {
	state: ModalState,
}

impl StoryComponent for ConfirmDialog {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let modal = Modal::new("Confirm Action")
			.content("Are you sure you want to proceed? This action cannot be undone.")
			.button("Cancel", false)
			.button("Confirm", true)
			.size(60, 30);
		modal.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Left | KeyCode::Right | KeyCode::Tab => self.state.toggle_button(),
			_ => {}
		}
	}
}

struct InfoDialog {
	state: ModalState,
}

impl StoryComponent for InfoDialog {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let modal = Modal::new("Information")
			.content("This is an informational message to the user.\n\nPress OK to continue.")
			.button("OK", true)
			.size(50, 25);
		modal.render(area, frame.buffer_mut(), &mut self.state);
	}
}

pub fn modal_story() -> Story {
	Story::new("Modal", "Dialog overlay with buttons")
		.variant("Confirm Dialog", ConfirmDialog { state: ModalState::new() })
		.variant("Info Dialog", InfoDialog { state: ModalState::new() })
}
