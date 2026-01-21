// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use loom_tui_widget_tool_panel::{ToolExecution, ToolPanel, ToolPanelState, ToolStatus};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct EmptyToolPanel {
	state: ToolPanelState,
}

impl StoryComponent for EmptyToolPanel {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let panel = ToolPanel::new(vec![]);
		panel.render(area, frame.buffer_mut(), &mut self.state);
	}
}

struct MixedStatusToolPanel {
	state: ToolPanelState,
}

impl StoryComponent for MixedStatusToolPanel {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let executions = vec![
			ToolExecution::new("read_file", ToolStatus::Success)
				.with_duration(42),
			ToolExecution::new("write_file", ToolStatus::Running),
			ToolExecution::new("bash", ToolStatus::Pending),
			ToolExecution::new("search", ToolStatus::Error)
				.with_output("File not found")
				.with_duration(15),
		];
		let panel = ToolPanel::new(executions).title("Tool Executions");
		panel.render(area, frame.buffer_mut(), &mut self.state);
	}
}

pub fn tool_panel_story() -> Story {
	Story::new("ToolPanel", "Panel showing tool executions")
		.variant("Empty", EmptyToolPanel { state: ToolPanelState::default() })
		.variant("Mixed Status", MixedStatusToolPanel { state: ToolPanelState::default() })
}
