// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use ratatui::Frame;
use ratatui::layout::Rect;

use loom_tui_core::{Action, ComponentError, Event, FocusState};
use loom_tui_theme::Theme;

pub use loom_tui_core::{LocaleContext, TextDirection};
pub use loom_tui_theme::LayoutDirection;

pub struct RenderContext<'a> {
	pub theme: &'a Theme,
	pub focus: &'a FocusState,
	pub locale: &'a LocaleContext,
}

impl<'a> RenderContext<'a> {
	pub fn new(theme: &'a Theme, focus: &'a FocusState, locale: &'a LocaleContext) -> Self {
		Self { theme, focus, locale }
	}

	pub fn direction(&self) -> TextDirection {
		self.locale.direction
	}

	pub fn layout(&self) -> LayoutDirection {
		LayoutDirection::new(self.locale.direction)
	}

	pub fn is_rtl(&self) -> bool {
		self.locale.is_rtl()
	}

	pub fn t(&self, key: &str) -> String {
		self.locale.t(key)
	}
}

/// Core trait for TUI components.
///
/// Components are the building blocks of the TUI. They handle events,
/// produce actions, and render themselves to the terminal.
pub trait Component: Send + Sync {
	fn id(&self) -> &str;

	/// Called once when the component is attached to the UI tree ("on_mount").
	/// Use this for one-time initialization that requires the component to be fully constructed.
	fn init(&mut self) -> Result<(), ComponentError> {
		Ok(())
	}

	fn handle_event(&mut self, event: &Event) -> Vec<Action>;

	fn update(&mut self, action: &Action) -> Vec<Action>;

	fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext);

	fn focusable(&self) -> bool {
		true
	}
}

/// Trait for components that separate their state from their logic.
///
/// This is useful for components where state needs to be managed externally,
/// such as in a parent component or application state.
pub trait StatefulComponent: Send + Sync {
	type State: Default;

	fn id(&self) -> &str;

	/// Called once when the component is attached to the UI tree ("on_mount").
	/// Use this for one-time initialization that requires the component to be fully constructed.
	fn init(&mut self, _state: &mut Self::State) -> Result<(), ComponentError> {
		Ok(())
	}

	fn handle_event(&mut self, event: &Event, state: &mut Self::State) -> Vec<Action>;

	fn update(&mut self, action: &Action, state: &mut Self::State) -> Vec<Action>;

	fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext, state: &Self::State);

	fn focusable(&self, _state: &Self::State) -> bool {
		true
	}
}

/// Extension trait for Elm-like action mapping composition.
pub trait ComponentExt: Component + Sized {
	fn map_actions<F>(self, f: F) -> MapActions<Self, F>
	where
		F: Fn(Action) -> Action + Clone,
	{
		MapActions { inner: self, f }
	}
}

impl<T: Component> ComponentExt for T {}

pub struct MapActions<C, F> {
	inner: C,
	f: F,
}

impl<C, F> Component for MapActions<C, F>
where
	C: Component,
	F: Fn(Action) -> Action + Clone + Send + Sync,
{
	fn id(&self) -> &str {
		self.inner.id()
	}

	fn init(&mut self) -> Result<(), ComponentError> {
		self.inner.init()
	}

	fn handle_event(&mut self, event: &Event) -> Vec<Action> {
		self.inner.handle_event(event).into_iter().map(&self.f).collect()
	}

	fn update(&mut self, action: &Action) -> Vec<Action> {
		self.inner.update(action).into_iter().map(&self.f).collect()
	}

	fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext) {
		self.inner.render(frame, area, ctx)
	}

	fn focusable(&self) -> bool {
		self.inner.focusable()
	}
}
