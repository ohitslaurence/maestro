// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use loom_tui_widget_spinner::{Spinner, SpinnerState};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct DefaultSpinner {
	state: SpinnerState,
}

impl StoryComponent for DefaultSpinner {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let spinner = Spinner::new();
		spinner.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn tick(&mut self) {
		self.state.tick();
	}
}

struct LabeledSpinner {
	state: SpinnerState,
}

impl StoryComponent for LabeledSpinner {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let spinner = Spinner::new().label("Loading...");
		spinner.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn tick(&mut self) {
		self.state.tick();
	}
}

pub fn spinner_story() -> Story {
	Story::new("Spinner", "Animated loading spinner widget")
		.variant("Default", DefaultSpinner { state: SpinnerState::default() })
		.variant("With Label", LabeledSpinner { state: SpinnerState::default() })
}
