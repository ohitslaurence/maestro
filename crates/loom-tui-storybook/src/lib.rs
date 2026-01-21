// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

pub mod stories;

pub trait StoryComponent: Send {
	fn render(&mut self, frame: &mut Frame, area: Rect);
	fn handle_key(&mut self, _key: KeyEvent) {}
	fn tick(&mut self) {}
}

pub struct StoryVariant {
	pub name: &'static str,
	pub component: Box<dyn StoryComponent>,
}

pub struct Story {
	pub name: &'static str,
	pub description: &'static str,
	pub variants: Vec<StoryVariant>,
}

impl Story {
	pub fn new(name: &'static str, description: &'static str) -> Self {
		Self {
			name,
			description,
			variants: Vec::new(),
		}
	}

	pub fn variant(mut self, name: &'static str, component: impl StoryComponent + 'static) -> Self {
		self.variants.push(StoryVariant {
			name,
			component: Box::new(component),
		});
		self
	}
}

#[derive(Default)]
pub struct StoryRegistry {
	stories: Vec<Story>,
}

impl StoryRegistry {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn register(&mut self, story: Story) {
		self.stories.push(story);
	}

	pub fn stories(&self) -> &[Story] {
		&self.stories
	}

	pub fn stories_mut(&mut self) -> &mut [Story] {
		&mut self.stories
	}

	pub fn get(&self, index: usize) -> Option<&Story> {
		self.stories.get(index)
	}

	pub fn get_mut(&mut self, index: usize) -> Option<&mut Story> {
		self.stories.get_mut(index)
	}

	pub fn len(&self) -> usize {
		self.stories.len()
	}

	pub fn is_empty(&self) -> bool {
		self.stories.is_empty()
	}
}

pub trait TuiApp {
	fn render(&mut self, frame: &mut Frame);
	fn on_key(&mut self, key: KeyEvent);
	fn on_tick(&mut self);
	fn should_quit(&self) -> bool;
}

pub fn run_tui_app<A: TuiApp>(mut app: A, tick_rate: Duration) -> anyhow::Result<()> {
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;

	let backend = CrosstermBackend::new(io::stdout());
	let mut terminal = Terminal::new(backend)?;

	let result = (|| -> anyhow::Result<()> {
		loop {
			terminal.draw(|frame| app.render(frame))?;

			if event::poll(tick_rate)? {
				if let Event::Key(key) = event::read()? {
					if key.kind == KeyEventKind::Press {
						app.on_key(key);
					}
				}
			}

			app.on_tick();

			if app.should_quit() {
				break;
			}
		}
		Ok(())
	})();

	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;

	result
}
