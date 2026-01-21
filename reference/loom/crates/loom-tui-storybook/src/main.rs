// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use loom_tui_core::LocaleContext;
use loom_tui_storybook::{run_tui_app, stories, StoryRegistry, TuiApp};
use ratatui::{
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Clear, Paragraph, Widget},
	Frame,
};

enum View {
	StoryList,
	VariantList { story_idx: usize },
	Preview { story_idx: usize, variant_idx: usize },
}

struct App {
	registry: StoryRegistry,
	view: View,
	story_cursor: usize,
	variant_cursor: usize,
	should_quit: bool,
	locale: LocaleContext,
}

impl App {
	fn new() -> Self {
		let mut registry = StoryRegistry::new();
		stories::register_all(&mut registry);

		Self {
			registry,
			view: View::StoryList,
			story_cursor: 0,
			variant_cursor: 0,
			should_quit: false,
			locale: LocaleContext::default(),
		}
	}

	fn handle_navigation(&mut self, code: KeyCode) {
		match code {
			KeyCode::Char('q') => self.should_quit = true,
			KeyCode::Char('j') | KeyCode::Down => self.move_down(),
			KeyCode::Char('k') | KeyCode::Up => self.move_up(),
			KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(),
			KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => self.back(),
			KeyCode::Char('L') => self.cycle_locale(),
			_ => {}
		}
	}

	fn cycle_locale(&mut self) {
		let new_locale = match self.locale.locale.as_str() {
			"en" => "ar",
			"ar" => "he",
			"he" => "en",
			_ => "en",
		};
		self.locale = LocaleContext::new(new_locale);
	}

	fn move_down(&mut self) {
		match &self.view {
			View::StoryList => {
				if self.story_cursor < self.registry.len().saturating_sub(1) {
					self.story_cursor += 1;
				}
			}
			View::VariantList { story_idx } => {
				if let Some(story) = self.registry.get(*story_idx) {
					if self.variant_cursor < story.variants.len().saturating_sub(1) {
						self.variant_cursor += 1;
					}
				}
			}
			View::Preview { .. } => {}
		}
	}

	fn move_up(&mut self) {
		match &self.view {
			View::StoryList => {
				self.story_cursor = self.story_cursor.saturating_sub(1);
			}
			View::VariantList { .. } => {
				self.variant_cursor = self.variant_cursor.saturating_sub(1);
			}
			View::Preview { .. } => {}
		}
	}

	fn enter(&mut self) {
		match &self.view {
			View::StoryList => {
				if self.registry.get(self.story_cursor).is_some() {
					self.variant_cursor = 0;
					self.view = View::VariantList {
						story_idx: self.story_cursor,
					};
				}
			}
			View::VariantList { story_idx } => {
				if let Some(story) = self.registry.get(*story_idx) {
					if self.variant_cursor < story.variants.len() {
						self.view = View::Preview {
							story_idx: *story_idx,
							variant_idx: self.variant_cursor,
						};
					}
				}
			}
			View::Preview { .. } => {}
		}
	}

	fn back(&mut self) {
		match &self.view {
			View::StoryList => {}
			View::VariantList { .. } => {
				self.view = View::StoryList;
			}
			View::Preview { story_idx, .. } => {
				self.view = View::VariantList {
					story_idx: *story_idx,
				};
			}
		}
	}
}

impl TuiApp for App {
	fn render(&mut self, frame: &mut Frame) {
		let area = frame.area();

		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
			.split(area);

		draw_sidebar(frame, chunks[0], self);
		draw_preview(frame, chunks[1], self);
	}

	fn on_key(&mut self, key: KeyEvent) {
		if let View::Preview {
			story_idx,
			variant_idx,
		} = &self.view
		{
			match key.code {
				KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('q') => {
					self.handle_navigation(key.code);
				}
				_ => {
					let story_idx = *story_idx;
					let variant_idx = *variant_idx;
					if let Some(story) = self.registry.get_mut(story_idx) {
						if let Some(variant) = story.variants.get_mut(variant_idx) {
							variant.component.handle_key(key);
						}
					}
				}
			}
		} else {
			self.handle_navigation(key.code);
		}
	}

	fn on_tick(&mut self) {
		if let View::Preview {
			story_idx,
			variant_idx,
		} = &self.view
		{
			let story_idx = *story_idx;
			let variant_idx = *variant_idx;
			if let Some(story) = self.registry.get_mut(story_idx) {
				if let Some(variant) = story.variants.get_mut(variant_idx) {
					variant.component.tick();
				}
			}
		}
	}

	fn should_quit(&self) -> bool {
		self.should_quit
	}
}

fn main() -> Result<()> {
	let app = App::new();
	run_tui_app(app, Duration::from_millis(100))
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App) {
	let block = Block::default()
		.borders(Borders::ALL)
		.title(" Components ")
		.border_style(Style::default().fg(Color::DarkGray));

	let inner = block.inner(area);
	frame.render_widget(block, area);

	match &app.view {
		View::StoryList => {
			let mut lines: Vec<Line> = Vec::new();
			for (i, story) in app.registry.stories().iter().enumerate() {
				let style = if i == app.story_cursor {
					Style::default()
						.fg(Color::Cyan)
						.add_modifier(Modifier::BOLD | Modifier::REVERSED)
				} else {
					Style::default()
				};
				lines.push(Line::from(Span::styled(format!(" {} ", story.name), style)));
			}
			let paragraph = Paragraph::new(lines);
			frame.render_widget(paragraph, inner);
		}
		View::VariantList { story_idx } | View::Preview { story_idx, .. } => {
			let mut lines: Vec<Line> = Vec::new();
			if let Some(story) = app.registry.get(*story_idx) {
				lines.push(Line::from(Span::styled(
					format!("◀ {}", story.name),
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
				)));
				lines.push(Line::from(""));

				for (i, variant) in story.variants.iter().enumerate() {
					let is_selected = matches!(&app.view, View::VariantList { .. }) && i == app.variant_cursor;
					let is_previewing = matches!(&app.view, View::Preview { variant_idx, .. } if *variant_idx == i);

					let style = if is_selected {
						Style::default()
							.fg(Color::Cyan)
							.add_modifier(Modifier::BOLD | Modifier::REVERSED)
					} else if is_previewing {
						Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
					} else {
						Style::default().fg(Color::White)
					};
					lines.push(Line::from(Span::styled(format!("  {} ", variant.name), style)));
				}
			}
			let paragraph = Paragraph::new(lines);
			frame.render_widget(paragraph, inner);
		}
	}

	let help_area = Rect::new(area.x, area.y + area.height.saturating_sub(1), area.width, 1);
	let locale_indicator = format!("[{}]", app.locale.locale);
	let help_text = format!(" j/k:nav  ↵:select  esc:back  L:locale  q:quit  {} ", locale_indicator);
	let help = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, help_area);
}

fn draw_preview(frame: &mut Frame, area: Rect, app: &mut App) {
	let (title, description) = match &app.view {
		View::StoryList => ("Preview".to_string(), "Select a component to preview"),
		View::VariantList { story_idx } => {
			if let Some(story) = app.registry.get(*story_idx) {
				(story.name.to_string(), story.description)
			} else {
				("Preview".to_string(), "")
			}
		}
		View::Preview {
			story_idx,
			variant_idx,
		} => {
			if let Some(story) = app.registry.get(*story_idx) {
				if let Some(variant) = story.variants.get(*variant_idx) {
					(format!("{} / {}", story.name, variant.name), story.description)
				} else {
					(story.name.to_string(), story.description)
				}
			} else {
				("Preview".to_string(), "")
			}
		}
	};

	let block = Block::default()
		.borders(Borders::ALL)
		.title(format!(" {} ", title))
		.border_style(Style::default().fg(Color::DarkGray));

	let inner = block.inner(area);
	frame.render_widget(block, area);

	match &mut app.view {
		View::StoryList => {
			let welcome = vec![
				Line::from(""),
				Line::from(Span::styled(
					"Loom TUI Storybook",
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
				)),
				Line::from(""),
				Line::from("Use j/k or arrow keys to navigate"),
				Line::from("Press Enter to select a component"),
				Line::from("Press q to quit"),
			];
			let paragraph = Paragraph::new(welcome);
			frame.render_widget(paragraph, inner);
		}
		View::VariantList { story_idx } => {
			if let Some(story) = app.registry.get(*story_idx) {
				let lines = vec![
					Line::from(""),
					Line::from(Span::styled(
						story.name,
						Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
					)),
					Line::from(""),
					Line::from(story.description),
					Line::from(""),
					Line::from("Select a variant to preview"),
				];
				let paragraph = Paragraph::new(lines);
				frame.render_widget(paragraph, inner);
			}
		}
		View::Preview {
			story_idx,
			variant_idx,
		} => {
			Clear.render(inner, frame.buffer_mut());

			let story_idx = *story_idx;
			let variant_idx = *variant_idx;
			if let Some(story) = app.registry.get_mut(story_idx) {
				if let Some(variant) = story.variants.get_mut(variant_idx) {
					let preview_area = Rect::new(
						inner.x + 1,
						inner.y + 1,
						inner.width.saturating_sub(2),
						inner.height.saturating_sub(2),
					);
					variant.component.render(frame, preview_area);
				}
			}
		}
	}

	if !description.is_empty() {
		let desc_area = Rect::new(area.x + 1, area.y + area.height.saturating_sub(1), area.width.saturating_sub(2), 1);
		let desc = Paragraph::new(description).style(Style::default().fg(Color::DarkGray));
		frame.render_widget(desc, desc_area);
	}
}
