// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

mod app;
mod layout;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::KeyEvent;
use loom_tui_storybook::{run_tui_app, TuiApp};
use ratatui::Frame;

use app::App;

const TICK_RATE: Duration = Duration::from_millis(100);

struct AppWrapper(App);

impl TuiApp for AppWrapper {
	fn render(&mut self, frame: &mut Frame) {
		self.0.render(frame);
	}

	fn on_key(&mut self, key: KeyEvent) {
		self.0.handle_key_event(key);
	}

	fn on_tick(&mut self) {
		self.0.tick();
	}

	fn should_quit(&self) -> bool {
		self.0.should_quit()
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let app = AppWrapper(App::new());
	run_tui_app(app, TICK_RATE)
}
