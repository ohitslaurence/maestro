// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::borrow::Cow;

use async_trait::async_trait;
use crossterm::event::{KeyEvent, MouseEvent};
use thiserror::Error;

pub use loom_common_i18n::{available_locales, is_rtl, resolve_locale, t, t_fmt, Direction, LocaleInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
	#[default]
	Ltr,
	Rtl,
}

impl From<loom_common_i18n::Direction> for TextDirection {
	fn from(dir: loom_common_i18n::Direction) -> Self {
		match dir {
			loom_common_i18n::Direction::Ltr => TextDirection::Ltr,
			loom_common_i18n::Direction::Rtl => TextDirection::Rtl,
		}
	}
}

impl TextDirection {
	pub fn from_locale(locale: &str) -> Self {
		if loom_common_i18n::is_rtl(locale) {
			TextDirection::Rtl
		} else {
			TextDirection::Ltr
		}
	}

	pub fn is_rtl(&self) -> bool {
		matches!(self, TextDirection::Rtl)
	}

	pub fn is_ltr(&self) -> bool {
		matches!(self, TextDirection::Ltr)
	}

	/// Mirror a horizontal position for RTL
	pub fn mirror_x(&self, x: u16, width: u16) -> u16 {
		match self {
			TextDirection::Ltr => x,
			TextDirection::Rtl => width.saturating_sub(x + 1),
		}
	}

	/// Get alignment start position
	pub fn align_start(&self, area_width: u16, content_width: u16) -> u16 {
		match self {
			TextDirection::Ltr => 0,
			TextDirection::Rtl => area_width.saturating_sub(content_width),
		}
	}

	/// Get alignment end position
	pub fn align_end(&self, area_width: u16, content_width: u16) -> u16 {
		match self {
			TextDirection::Ltr => area_width.saturating_sub(content_width),
			TextDirection::Rtl => 0,
		}
	}
}

#[derive(Debug, Clone)]
pub struct LocaleContext {
	pub locale: String,
	pub direction: TextDirection,
}

impl Default for LocaleContext {
	fn default() -> Self {
		Self {
			locale: "en".to_string(),
			direction: TextDirection::Ltr,
		}
	}
}

impl LocaleContext {
	pub fn new(locale: impl Into<String>) -> Self {
		let locale = locale.into();
		let direction = TextDirection::from_locale(&locale);
		Self { locale, direction }
	}

	pub fn is_rtl(&self) -> bool {
		self.direction.is_rtl()
	}

	/// Translate a key using this context's locale
	pub fn t(&self, key: &str) -> String {
		loom_common_i18n::t(&self.locale, key)
	}

	/// Translate a key with format variables
	pub fn t_fmt(&self, key: &str, vars: &[(&str, &str)]) -> String {
		loom_common_i18n::t_fmt(&self.locale, key, vars)
	}
}

/// Result type alias using ComponentError as the default error type.
pub type Result<T, E = ComponentError> = std::result::Result<T, E>;

/// Type alias for focus identifiers.
pub type FocusId = String;

/// Actions that components can emit in response to events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
	/// Periodic tick for animations or polling.
	Tick,
	/// Request application shutdown.
	Quit,
	/// Request a re-render of the UI.
	Render,
	/// Terminal was resized to (width, height).
	Resize(u16, u16),
	/// Move focus to the next focusable component.
	FocusNext,
	/// Move focus to the previous focusable component.
	FocusPrev,
	/// Scroll up by the specified number of lines.
	ScrollUp(usize),
	/// Scroll down by the specified number of lines.
	ScrollDown(usize),
	/// Submit/confirm the current input or selection.
	Submit,
	/// Cancel the current operation.
	Cancel,
	/// Custom action with a kind identifier and payload.
	Custom { kind: Cow<'static, str>, payload: String },
}

/// Terminal events from crossterm.
#[derive(Debug, Clone)]
pub enum Event {
	/// Keyboard input.
	Key(KeyEvent),
	/// Mouse input.
	Mouse(MouseEvent),
	/// Terminal resize to (width, height).
	Resize(u16, u16),
	/// Periodic tick.
	Tick,
	/// Paste event with pasted text.
	Paste(String),
	/// Terminal gained focus.
	FocusGained,
	/// Terminal lost focus.
	FocusLost,
}

impl From<crossterm::event::Event> for Event {
	fn from(event: crossterm::event::Event) -> Self {
		match event {
			crossterm::event::Event::Key(key) => Event::Key(key),
			crossterm::event::Event::Mouse(mouse) => Event::Mouse(mouse),
			crossterm::event::Event::Resize(w, h) => Event::Resize(w, h),
			crossterm::event::Event::Paste(text) => Event::Paste(text),
			crossterm::event::Event::FocusGained => Event::FocusGained,
			crossterm::event::Event::FocusLost => Event::FocusLost,
		}
	}
}

/// Outcome of event handling, indicating whether the event was consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome<A> {
	/// Event was not handled; propagate to other handlers.
	Ignored,
	/// Event was handled, optionally producing an action.
	Handled(Option<A>),
}

/// Tracks which component currently has focus.
#[derive(Debug, Default, Clone)]
pub struct FocusState {
	pub focused_id: Option<FocusId>,
	pub focusable_ids: Vec<FocusId>,
}

impl FocusState {
	/// Register a new focusable component.
	pub fn register(&mut self, id: FocusId) {
		if !self.focusable_ids.contains(&id) {
			self.focusable_ids.push(id);
		}
	}

	/// Unregister a focusable component.
	pub fn unregister(&mut self, id: &str) {
		self.focusable_ids.retain(|i| i != id);
		if self.focused_id.as_deref() == Some(id) {
			self.focused_id = None;
		}
	}

	/// Move focus to the next component.
	pub fn focus_next(&mut self) {
		if self.focusable_ids.is_empty() {
			return;
		}
		let next_idx = match self.focused_index() {
			Some(idx) => (idx + 1) % self.focusable_ids.len(),
			None => 0,
		};
		self.focused_id = Some(self.focusable_ids[next_idx].clone());
	}

	/// Move focus to the previous component.
	pub fn focus_prev(&mut self) {
		if self.focusable_ids.is_empty() {
			return;
		}
		let prev_idx = match self.focused_index() {
			Some(idx) => {
				if idx == 0 {
					self.focusable_ids.len() - 1
				} else {
					idx - 1
				}
			}
			None => self.focusable_ids.len() - 1,
		};
		self.focused_id = Some(self.focusable_ids[prev_idx].clone());
	}

	/// Set focus to a specific component by id.
	pub fn set_focus(&mut self, id: &str) {
		if self.focusable_ids.iter().any(|i| i == id) {
			self.focused_id = Some(id.to_string());
		}
	}

	/// Check if a component has focus.
	pub fn is_focused(&self, id: &str) -> bool {
		self.focused_id.as_deref() == Some(id)
	}

	fn focused_index(&self) -> Option<usize> {
		self.focused_id
			.as_ref()
			.and_then(|id| self.focusable_ids.iter().position(|i| i == id))
	}
}

/// Trait for mapping key events to actions based on focus state.
pub trait Keymap<A> {
	fn key_to_action(&self, key: &KeyEvent, focus: &FocusState) -> Option<A>;
}

#[async_trait]
pub trait EventSource {
	async fn next(&mut self) -> Option<Event>;
}

#[derive(Debug, Error)]
pub enum ComponentError {
	#[error("initialization failed: {0}")]
	Init(String),
	#[error("render failed: {0}")]
	Render(String),
}
