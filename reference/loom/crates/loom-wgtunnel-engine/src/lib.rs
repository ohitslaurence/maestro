// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod device;
pub mod engine;
pub mod error;
pub mod peers;
pub mod router;

pub use device::{VirtualDevice, VirtualTcpListener, VirtualTcpStream};
pub use engine::{WgEngine, WgEngineConfig};
pub use error::{EngineError, Result};
pub use peers::{PeerConfig, PeerManager, PeerState};
pub use router::Router;
