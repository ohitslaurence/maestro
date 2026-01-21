// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, PartialEq)]
pub struct ColorPalette {
	pub background: Color,
	pub surface: Color,
	pub surface_alt: Color,
	pub text: Color,
	pub text_muted: Color,
	pub text_placeholder: Color,
	pub text_inverted: Color,
	pub accent: Color,
	pub accent_soft: Color,
	pub error: Color,
	pub warning: Color,
	pub success: Color,
	pub selection_bg: Color,
	pub selection_fg: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BorderStyles {
	pub normal: Style,
	pub focused: Style,
	pub error: Style,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyles {
	pub normal: Style,
	pub bold: Style,
	pub dim: Style,
	pub italic: Style,
	pub code: Style,
	pub link: Style,
	pub placeholder: Style,
	pub disabled: Style,
	pub error: Style,
	pub success: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spacing {
	pub padding: u16,
	pub padding_dense: u16,
	pub gutter: u16,
	pub margin: u16,
}

impl Spacing {
	pub fn new() -> Self {
		Self {
			padding: 1,
			padding_dense: 0,
			gutter: 1,
			margin: 0,
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
	pub name: String,
	pub colors: ColorPalette,
	pub borders: BorderStyles,
	pub text: TextStyles,
	pub spacing: Spacing,
}

impl Default for Theme {
	fn default() -> Self {
		Self::dark()
	}
}

impl Theme {
	pub fn dark() -> Self {
		let colors = ColorPalette {
			background: Color::Black,
			surface: Color::Rgb(30, 30, 30),
			surface_alt: Color::Rgb(45, 45, 45),
			text: Color::White,
			text_muted: Color::DarkGray,
			text_placeholder: Color::Rgb(100, 100, 100),
			text_inverted: Color::Black,
			accent: Color::Cyan,
			accent_soft: Color::Rgb(0, 100, 100),
			error: Color::Red,
			warning: Color::Yellow,
			success: Color::Green,
			selection_bg: Color::Blue,
			selection_fg: Color::White,
		};

		let borders = BorderStyles {
			normal: Style::default().fg(colors.text_muted),
			focused: Style::default().fg(colors.accent),
			error: Style::default().fg(colors.error),
		};

		let text = TextStyles {
			normal: Style::default().fg(colors.text),
			bold: Style::default().fg(colors.text).add_modifier(Modifier::BOLD),
			dim: Style::default().fg(colors.text_muted),
			italic: Style::default().fg(colors.text).add_modifier(Modifier::ITALIC),
			code: Style::default().fg(colors.accent),
			link: Style::default().fg(colors.accent).add_modifier(Modifier::UNDERLINED),
			placeholder: Style::default().fg(colors.text_placeholder),
			disabled: Style::default().fg(colors.text_muted),
			error: Style::default().fg(colors.error),
			success: Style::default().fg(colors.success),
		};

		Self {
			name: "dark".to_string(),
			colors,
			borders,
			text,
			spacing: Spacing::new(),
		}
	}

	pub fn light() -> Self {
		let colors = ColorPalette {
			background: Color::White,
			surface: Color::Rgb(245, 245, 245),
			surface_alt: Color::Rgb(230, 230, 230),
			text: Color::Black,
			text_muted: Color::Gray,
			text_placeholder: Color::Rgb(160, 160, 160),
			text_inverted: Color::White,
			accent: Color::Blue,
			accent_soft: Color::Rgb(200, 220, 255),
			error: Color::Red,
			warning: Color::Yellow,
			success: Color::Green,
			selection_bg: Color::LightBlue,
			selection_fg: Color::Black,
		};

		let borders = BorderStyles {
			normal: Style::default().fg(colors.text_muted),
			focused: Style::default().fg(colors.accent),
			error: Style::default().fg(colors.error),
		};

		let text = TextStyles {
			normal: Style::default().fg(colors.text),
			bold: Style::default().fg(colors.text).add_modifier(Modifier::BOLD),
			dim: Style::default().fg(colors.text_muted),
			italic: Style::default().fg(colors.text).add_modifier(Modifier::ITALIC),
			code: Style::default().fg(colors.accent),
			link: Style::default().fg(colors.accent).add_modifier(Modifier::UNDERLINED),
			placeholder: Style::default().fg(colors.text_placeholder),
			disabled: Style::default().fg(colors.text_muted),
			error: Style::default().fg(colors.error),
			success: Style::default().fg(colors.success),
		};

		Self {
			name: "light".to_string(),
			colors,
			borders,
			text,
			spacing: Spacing::new(),
		}
	}

	pub fn border_normal(&self) -> Style {
		self.borders.normal
	}

	pub fn border_focused(&self) -> Style {
		self.borders.focused
	}

	pub fn border_error(&self) -> Style {
		self.borders.error
	}

	pub fn selection_style(&self) -> Style {
		Style::default().bg(self.colors.selection_bg).fg(self.colors.selection_fg)
	}

	pub fn input_text(&self) -> Style {
		self.text.normal
	}

	pub fn input_placeholder(&self) -> Style {
		self.text.placeholder
	}

	pub fn input_disabled(&self) -> Style {
		self.text.disabled
	}

	pub fn error_text(&self) -> Style {
		self.text.error
	}

	pub fn success_text(&self) -> Style {
		self.text.success
	}

	pub fn border_style_for(&self, focused: bool, _direction: TextDirection) -> Style {
		if focused {
			self.borders.focused
		} else {
			self.borders.normal
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutDirection {
	pub direction: TextDirection,
}

impl Default for LayoutDirection {
	fn default() -> Self {
		Self {
			direction: TextDirection::Ltr,
		}
	}
}

impl LayoutDirection {
	pub fn new(direction: TextDirection) -> Self {
		Self { direction }
	}

	pub fn from_locale(locale: &str) -> Self {
		Self {
			direction: TextDirection::from_locale(locale),
		}
	}

	pub fn is_rtl(&self) -> bool {
		self.direction.is_rtl()
	}

	pub fn start_x(&self, area_x: u16, area_width: u16, content_width: u16) -> u16 {
		if self.is_rtl() {
			area_x + area_width.saturating_sub(content_width)
		} else {
			area_x
		}
	}

	pub fn end_x(&self, area_x: u16, area_width: u16, content_width: u16) -> u16 {
		if self.is_rtl() {
			area_x
		} else {
			area_x + area_width.saturating_sub(content_width)
		}
	}

	pub fn split_horizontal(
		&self,
		area: ratatui::layout::Rect,
		start_width: u16,
	) -> (ratatui::layout::Rect, ratatui::layout::Rect) {
		use ratatui::layout::Rect;

		let end_width = area.width.saturating_sub(start_width);

		if self.is_rtl() {
			let end_area = Rect::new(area.x, area.y, end_width, area.height);
			let start_area = Rect::new(area.x + end_width, area.y, start_width, area.height);
			(start_area, end_area)
		} else {
			let start_area = Rect::new(area.x, area.y, start_width, area.height);
			let end_area = Rect::new(area.x + start_width, area.y, end_width, area.height);
			(start_area, end_area)
		}
	}
}
