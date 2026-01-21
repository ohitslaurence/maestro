// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::PeerError;
use eventsource_stream::Eventsource;
use futures::stream::BoxStream;
use futures::StreamExt;
use loom_common_secret::SecretString;
use serde::Deserialize;
use tracing::{debug, instrument, warn};
use url::Url;

pub struct PeerHandler {
	event_stream: Option<BoxStream<'static, Result<PeerEvent, PeerError>>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum PeerEvent {
	#[serde(rename = "peer_added")]
	PeerAdded {
		public_key: String,
		allowed_ip: String,
		session_id: String,
	},
	#[serde(rename = "peer_removed")]
	PeerRemoved {
		public_key: String,
		session_id: String,
	},
}

impl PeerHandler {
	pub fn new() -> Self {
		Self { event_stream: None }
	}

	#[instrument(skip(self, svid), fields(weaver_id = %weaver_id))]
	pub async fn connect(
		&mut self,
		server_url: &Url,
		weaver_id: &str,
		svid: &SecretString,
	) -> Result<(), PeerError> {
		let url = server_url
			.join(&format!("/internal/wg/weavers/{}/peers", weaver_id))
			.map_err(|e| PeerError::Connection(e.to_string()))?;

		debug!(%url, "connecting to peer stream");

		let client = loom_common_http::new_client();

		let response = client
			.get(url.clone())
			.header("Authorization", format!("Bearer {}", svid.expose()))
			.header("Accept", "text/event-stream")
			.send()
			.await?;

		if !response.status().is_success() {
			return Err(PeerError::Connection(format!(
				"server returned status {}",
				response.status()
			)));
		}

		let stream = response.bytes_stream().eventsource();

		let mapped_stream = stream
			.filter_map(|result| async move {
				match result {
					Ok(event) => {
						if event.event == "message" || event.event.is_empty() {
							match serde_json::from_str::<PeerEvent>(&event.data) {
								Ok(peer_event) => Some(Ok(peer_event)),
								Err(e) => {
									warn!(error = %e, data = %event.data, "failed to parse peer event");
									Some(Err(PeerError::Parse(e)))
								}
							}
						} else {
							None
						}
					}
					Err(e) => {
						warn!(error = %e, "SSE stream error");
						Some(Err(PeerError::Connection(e.to_string())))
					}
				}
			})
			.boxed();

		self.event_stream = Some(mapped_stream);

		debug!("connected to peer stream");

		Ok(())
	}

	pub async fn next(&mut self) -> Option<Result<PeerEvent, PeerError>> {
		if let Some(ref mut stream) = self.event_stream {
			stream.next().await
		} else {
			None
		}
	}

	pub fn close(&mut self) {
		self.event_stream = None;
	}
}

impl Default for PeerHandler {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Debug for PeerHandler {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PeerHandler")
			.field("connected", &self.event_stream.is_some())
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_peer_event_deserialize_added() {
		let json =
			r#"{"type":"peer_added","public_key":"abc123","allowed_ip":"fd7a::1","session_id":"sess-1"}"#;
		let event: PeerEvent = serde_json::from_str(json).unwrap();
		match event {
			PeerEvent::PeerAdded {
				public_key,
				allowed_ip,
				session_id,
			} => {
				assert_eq!(public_key, "abc123");
				assert_eq!(allowed_ip, "fd7a::1");
				assert_eq!(session_id, "sess-1");
			}
			_ => panic!("expected PeerAdded"),
		}
	}

	#[test]
	fn test_peer_event_deserialize_removed() {
		let json = r#"{"type":"peer_removed","public_key":"abc123","session_id":"sess-1"}"#;
		let event: PeerEvent = serde_json::from_str(json).unwrap();
		match event {
			PeerEvent::PeerRemoved {
				public_key,
				session_id,
			} => {
				assert_eq!(public_key, "abc123");
				assert_eq!(session_id, "sess-1");
			}
			_ => panic!("expected PeerRemoved"),
		}
	}

	#[test]
	fn test_peer_handler_default() {
		let handler = PeerHandler::default();
		assert!(handler.event_stream.is_none());
	}
}
