// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use loom_tui_theme::Theme;
use ratatui::{
	buffer::Buffer,
	layout::{Alignment, Rect},
	style::{Modifier, Style},
	text::Text,
	widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Debug, Clone)]
pub struct ModalButton {
	pub label: String,
	pub is_primary: bool,
}

impl ModalButton {
	pub fn new(label: impl Into<String>, is_primary: bool) -> Self {
		Self {
			label: label.into(),
			is_primary,
		}
	}
}

#[derive(Debug, Clone)]
pub struct Modal {
	title: String,
	content: Text<'static>,
	buttons: Vec<ModalButton>,
	width_percent: u16,
	height_percent: u16,
	theme: Theme,
	track_style: Style,
	thumb_style: Style,
	direction: TextDirection,
}

impl Modal {
	pub fn new(title: impl Into<String>) -> Self {
		Self {
			title: title.into(),
			content: Text::default(),
			buttons: Vec::new(),
			width_percent: 50,
			height_percent: 50,
			theme: Theme::default(),
			track_style: Style::default(),
			thumb_style: Style::default(),
			direction: TextDirection::default(),
		}
	}

	pub fn content(mut self, content: impl Into<Text<'static>>) -> Self {
		self.content = content.into();
		self
	}

	pub fn button(mut self, label: impl Into<String>, is_primary: bool) -> Self {
		self.buttons.push(ModalButton::new(label, is_primary));
		self
	}

	pub fn size(mut self, width_percent: u16, height_percent: u16) -> Self {
		self.width_percent = width_percent.clamp(10, 100);
		self.height_percent = height_percent.clamp(10, 100);
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn track_style(mut self, style: Style) -> Self {
		self.track_style = style;
		self
	}

	pub fn thumb_style(mut self, style: Style) -> Self {
		self.thumb_style = style;
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}

	fn centered_rect(&self, area: Rect) -> Rect {
		let width = (area.width as u32 * self.width_percent as u32 / 100) as u16;
		let height = (area.height as u32 * self.height_percent as u32 / 100) as u16;
		let x = area.x + (area.width.saturating_sub(width)) / 2;
		let y = area.y + (area.height.saturating_sub(height)) / 2;
		Rect::new(x, y, width, height)
	}
}

#[derive(Debug, Default, Clone)]
pub struct ModalState {
	selected_button: usize,
}

impl ModalState {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn select_next(&mut self, total: usize) {
		if total > 0 {
			self.selected_button = (self.selected_button + 1) % total;
		}
	}

	pub fn select_prev(&mut self, total: usize) {
		if total > 0 {
			self.selected_button = self.selected_button.checked_sub(1).unwrap_or(total - 1);
		}
	}

	pub fn selected(&self) -> usize {
		self.selected_button
	}

	pub fn toggle_button(&mut self) {
		self.selected_button = if self.selected_button == 0 { 1 } else { 0 };
	}
}

impl StatefulWidget for Modal {
	type State = ModalState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		let modal_area = self.centered_rect(area);

		if modal_area.width < 3 || modal_area.height < 3 {
			return;
		}

		let is_rtl = self.direction.is_rtl();

		for y in area.y..area.y + area.height {
			for x in area.x..area.x + area.width {
				if x < modal_area.x
					|| x >= modal_area.x + modal_area.width
					|| y < modal_area.y
					|| y >= modal_area.y + modal_area.height
				{
					buf[(x, y)].set_style(self.track_style);
				}
			}
		}

		Clear.render(modal_area, buf);

		let title_alignment = if is_rtl { Alignment::Right } else { Alignment::Left };

		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(self.theme.borders.normal)
			.title(self.title.clone())
			.title_alignment(title_alignment)
			.style(Style::default().bg(self.theme.colors.background).fg(self.theme.colors.text));

		let inner_area = block.inner(modal_area);
		block.render(modal_area, buf);

		if inner_area.width == 0 || inner_area.height == 0 {
			return;
		}

		let button_height = if self.buttons.is_empty() { 0 } else { 1 };
		let content_height = inner_area.height.saturating_sub(button_height + 1);

		if content_height > 0 {
			let content_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, content_height);
			let content_alignment = if is_rtl { Alignment::Right } else { Alignment::Left };
			let content = Paragraph::new(self.content.clone())
				.style(self.theme.text.normal)
				.alignment(content_alignment)
				.wrap(Wrap { trim: true });
			content.render(content_area, buf);
		}

		if !self.buttons.is_empty() && inner_area.height > 1 {
			let button_y = inner_area.y + inner_area.height - 1;
			let mut button_strs: Vec<String> = Vec::new();

			let buttons_to_render: Vec<_> = if is_rtl {
				self.buttons.iter().rev().collect()
			} else {
				self.buttons.iter().collect()
			};

			for (i, btn) in buttons_to_render.iter().enumerate() {
				let visual_selected = if is_rtl && !self.buttons.is_empty() {
					self.buttons.len() - 1 - state.selected_button
				} else {
					state.selected_button
				};
				let is_selected = i == visual_selected;
				let label = if is_selected {
					format!("[ {} ]", btn.label)
				} else {
					format!("  {}  ", btn.label)
				};
				button_strs.push(label);
			}

			let total_len: usize = button_strs.iter().map(|s| s.len()).sum::<usize>() + button_strs.len().saturating_sub(1);
			let start_x = inner_area.x + (inner_area.width.saturating_sub(total_len as u16)) / 2;

			let mut x = start_x;
			for (i, label) in button_strs.iter().enumerate() {
				let visual_selected = if is_rtl && !self.buttons.is_empty() {
					self.buttons.len() - 1 - state.selected_button
				} else {
					state.selected_button
				};
				let is_selected = i == visual_selected;
				let is_primary = buttons_to_render[i].is_primary;

				let style = if is_selected {
					if is_primary {
						Style::default()
							.fg(self.theme.colors.background)
							.bg(self.theme.colors.accent)
							.add_modifier(Modifier::BOLD)
					} else {
						Style::default()
							.fg(self.theme.colors.background)
							.bg(self.theme.colors.selection_bg)
							.add_modifier(Modifier::BOLD)
					}
				} else if is_primary {
					Style::default().fg(self.theme.colors.accent).add_modifier(Modifier::BOLD)
				} else {
					self.theme.text.normal
				};

				buf.set_string(x, button_y, label, style);
				x += label.len() as u16 + 1;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_modal_builder() {
		let modal = Modal::new("Test")
			.content("Hello world")
			.button("OK", true)
			.button("Cancel", false)
			.size(60, 40);

		assert_eq!(modal.title, "Test");
		assert_eq!(modal.buttons.len(), 2);
		assert_eq!(modal.width_percent, 60);
		assert_eq!(modal.height_percent, 40);
	}

	#[test]
	fn test_modal_size_clamping() {
		let modal = Modal::new("Test").size(5, 150);
		assert_eq!(modal.width_percent, 10);
		assert_eq!(modal.height_percent, 100);

		let modal2 = Modal::new("Test").size(200, 0);
		assert_eq!(modal2.width_percent, 100);
		assert_eq!(modal2.height_percent, 10);
	}

	#[test]
	fn test_modal_rich_content() {
		use ratatui::text::{Line, Span};
		let text = Text::from(vec![
			Line::from(vec![Span::raw("Line 1")]),
			Line::from(vec![Span::raw("Line 2")]),
		]);
		let modal = Modal::new("Test").content(text);
		assert_eq!(modal.content.lines.len(), 2);
	}

	#[test]
	fn test_modal_state_navigation() {
		let mut state = ModalState::new();
		assert_eq!(state.selected(), 0);

		state.select_next(3);
		assert_eq!(state.selected(), 1);

		state.select_next(3);
		assert_eq!(state.selected(), 2);

		state.select_next(3);
		assert_eq!(state.selected(), 0);

		state.select_prev(3);
		assert_eq!(state.selected(), 2);
	}

	#[test]
	fn test_modal_state_empty_buttons() {
		let mut state = ModalState::new();
		state.select_next(0);
		assert_eq!(state.selected(), 0);

		state.select_prev(0);
		assert_eq!(state.selected(), 0);
	}

	#[test]
	fn test_modal_direction() {
		let modal = Modal::new("Test").direction(TextDirection::Rtl);
		assert!(modal.direction.is_rtl());

		let modal_ltr = Modal::new("Test").direction(TextDirection::Ltr);
		assert!(!modal_ltr.direction.is_rtl());
	}
}
