// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

mod header;
mod input_box;
mod markdown;
mod message_list;
mod modal;
mod spinner;
mod status_bar;
mod thread_list;
mod tool_panel;

pub use header::header_story;
pub use input_box::input_box_story;
pub use markdown::markdown_story;
pub use message_list::message_list_story;
pub use modal::modal_story;
pub use spinner::spinner_story;
pub use status_bar::status_bar_story;
pub use thread_list::thread_list_story;
pub use tool_panel::tool_panel_story;

use crate::StoryRegistry;

pub fn register_all(registry: &mut StoryRegistry) {
	registry.register(spinner_story());
	registry.register(header_story());
	registry.register(status_bar_story());
	registry.register(input_box_story());
	registry.register(message_list_story());
	registry.register(thread_list_story());
	registry.register(tool_panel_story());
	registry.register(markdown_story());
	registry.register(modal_story());
}
