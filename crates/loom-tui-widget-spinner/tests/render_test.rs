// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_widget_spinner::{Spinner, SpinnerState};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use ratatui::widgets::StatefulWidget;

fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
	let buffer = terminal.backend().buffer();
	let area = buffer.area;
	let mut output = String::new();

	for y in area.y..area.y + area.height {
		for x in area.x..area.x + area.width {
			let cell = &buffer[(x, y)];
			output.push_str(cell.symbol());
		}
		if y < area.y + area.height - 1 {
			output.push('\n');
		}
	}

	output
}

#[test]
fn test_spinner_default() {
	let backend = TestBackend::new(10, 1);
	let mut terminal = Terminal::new(backend).unwrap();

	terminal
		.draw(|frame| {
			let mut state = SpinnerState::default();
			let spinner = Spinner::new();
			spinner.render(frame.area(), frame.buffer_mut(), &mut state);
		})
		.unwrap();

	let output = buffer_to_string(&terminal);
	insta::assert_snapshot!("spinner_default", output);
}

#[test]
fn test_spinner_with_label() {
	let backend = TestBackend::new(20, 1);
	let mut terminal = Terminal::new(backend).unwrap();

	terminal
		.draw(|frame| {
			let mut state = SpinnerState::default();
			let spinner = Spinner::new().label("Loading...");
			spinner.render(frame.area(), frame.buffer_mut(), &mut state);
		})
		.unwrap();

	let output = buffer_to_string(&terminal);
	insta::assert_snapshot!("spinner_with_label", output);
}
