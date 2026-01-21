// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::HashSet;

use loom_tui_core::TextDirection;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Color, Style},
	text::{Line, Span},
	widgets::StatefulWidget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
	Pending,
	Running,
	Success,
	Error,
}

impl ToolStatus {
	pub fn icon(&self) -> &'static str {
		match self {
			ToolStatus::Pending => "⏳",
			ToolStatus::Running => "⚙",
			ToolStatus::Success => "✓",
			ToolStatus::Error => "✗",
		}
	}
}

pub fn status_style(status: ToolStatus) -> Style {
	match status {
		ToolStatus::Pending => Style::default().fg(Color::Gray),
		ToolStatus::Running => Style::default().fg(Color::Yellow),
		ToolStatus::Success => Style::default().fg(Color::Green),
		ToolStatus::Error => Style::default().fg(Color::Red),
	}
}

#[derive(Debug, Clone)]
pub struct ToolExecution {
	pub name: String,
	pub status: ToolStatus,
	pub output: Option<String>,
	pub duration_ms: Option<u64>,
}

impl ToolExecution {
	pub fn new(name: impl Into<String>, status: ToolStatus) -> Self {
		Self {
			name: name.into(),
			status,
			output: None,
			duration_ms: None,
		}
	}

	pub fn with_output(mut self, output: impl Into<String>) -> Self {
		self.output = Some(output.into());
		self
	}

	pub fn with_duration(mut self, duration_ms: u64) -> Self {
		self.duration_ms = Some(duration_ms);
		self
	}
}

#[derive(Debug, Default, Clone)]
pub struct ToolPanelState {
	pub scroll_offset: usize,
	pub expanded: HashSet<usize>,
}

impl ToolPanelState {
	pub fn toggle_expanded(&mut self, index: usize) {
		if self.expanded.contains(&index) {
			self.expanded.remove(&index);
		} else {
			self.expanded.insert(index);
		}
	}

	pub fn collapse_all(&mut self) {
		self.expanded.clear();
	}

	pub fn scroll_up(&mut self, amount: usize) {
		self.scroll_offset = self.scroll_offset.saturating_sub(amount);
	}

	pub fn scroll_down(&mut self, amount: usize, total: usize) {
		if total > 0 {
			self.scroll_offset = self.scroll_offset.saturating_add(amount).min(total - 1);
		}
	}
}

#[derive(Debug, Clone)]
pub struct ToolPanel {
	executions: Vec<ToolExecution>,
	title: String,
	style: Style,
	direction: TextDirection,
}

impl ToolPanel {
	pub fn new(executions: Vec<ToolExecution>) -> Self {
		Self {
			executions,
			title: "Tools".to_string(),
			style: Style::default(),
			direction: TextDirection::Ltr,
		}
	}

	pub fn title(mut self, title: impl Into<String>) -> Self {
		self.title = title.into();
		self
	}

	pub fn style(mut self, style: Style) -> Self {
		self.style = style;
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}
}

impl StatefulWidget for ToolPanel {
	type State = ToolPanelState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		if area.height == 0 || area.width == 0 {
			return;
		}

		let mut y = area.y;
		let max_y = area.y + area.height;

		if !self.title.is_empty() {
			let title_line = Line::from(vec![Span::styled(&self.title, self.style)]);
			buf.set_line(area.x, y, &title_line, area.width);
			y += 1;

			if y >= max_y {
				return;
			}
		}

		let is_rtl = self.direction.is_rtl();

		for (idx, execution) in self.executions.iter().enumerate().skip(state.scroll_offset) {
			if y >= max_y {
				break;
			}

			let icon = execution.status.icon();
			let duration_str = execution
				.duration_ms
				.map(|d| format!(" ({}ms)", d))
				.unwrap_or_default();

			let has_output = execution.output.is_some();
			let is_expanded = state.expanded.contains(&idx);
			let marker = if has_output {
				if is_expanded { "▾ " } else { "▸ " }
			} else {
				"  "
			};

			let header = if is_rtl {
				format!("{} {} {} {}", duration_str.trim(), execution.name, icon, marker.trim())
			} else {
				format!("{}{} {}{}", marker, icon, execution.name, duration_str)
			};
			let header_style = status_style(execution.status);
			let header_line = Line::from(vec![Span::styled(&header, header_style)]);
			buf.set_line(area.x, y, &header_line, area.width);
			y += 1;

			if is_expanded {
				if let Some(output) = &execution.output {
					for line in output.lines() {
						if y >= max_y {
							break;
						}

						let indent = "  ";
						let content_line = Line::from(vec![Span::styled(
							format!("{}{}", indent, line),
							self.style,
						)]);
						buf.set_line(area.x, y, &content_line, area.width);
						y += 1;
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_tool_status_icons() {
		assert_eq!(ToolStatus::Pending.icon(), "⏳");
		assert_eq!(ToolStatus::Running.icon(), "⚙");
		assert_eq!(ToolStatus::Success.icon(), "✓");
		assert_eq!(ToolStatus::Error.icon(), "✗");
	}

	#[test]
	fn test_status_style() {
		let pending = status_style(ToolStatus::Pending);
		assert_eq!(pending.fg, Some(Color::Gray));

		let running = status_style(ToolStatus::Running);
		assert_eq!(running.fg, Some(Color::Yellow));

		let success = status_style(ToolStatus::Success);
		assert_eq!(success.fg, Some(Color::Green));

		let error = status_style(ToolStatus::Error);
		assert_eq!(error.fg, Some(Color::Red));
	}

	#[test]
	fn test_tool_execution_builder() {
		let exec = ToolExecution::new("read_file", ToolStatus::Success)
			.with_output("file contents here")
			.with_duration(42);

		assert_eq!(exec.name, "read_file");
		assert_eq!(exec.status, ToolStatus::Success);
		assert_eq!(exec.output, Some("file contents here".to_string()));
		assert_eq!(exec.duration_ms, Some(42));
	}

	#[test]
	fn test_tool_panel_state_toggle() {
		let mut state = ToolPanelState::default();
		assert!(!state.expanded.contains(&0));

		state.toggle_expanded(0);
		assert!(state.expanded.contains(&0));

		state.toggle_expanded(0);
		assert!(!state.expanded.contains(&0));
	}

	#[test]
	fn test_tool_panel_state_collapse_all() {
		let mut state = ToolPanelState::default();
		state.toggle_expanded(0);
		state.toggle_expanded(1);
		state.toggle_expanded(2);
		assert_eq!(state.expanded.len(), 3);

		state.collapse_all();
		assert!(state.expanded.is_empty());
	}

	#[test]
	fn test_tool_panel_state_scroll() {
		let mut state = ToolPanelState::default();
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
	fn test_tool_panel_builder() {
		let executions = vec![ToolExecution::new("test", ToolStatus::Pending)];
		let panel = ToolPanel::new(executions).title("My Tools");

		assert_eq!(panel.title, "My Tools");
		assert_eq!(panel.executions.len(), 1);
	}
}
