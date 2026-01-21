<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# WireGuard Tunnel System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-03

---

## Table of Contents

1. [Overview](#1-overview)
2. [Goals and Non-Goals](#2-goals-and-non-goals)
3. [Architecture](#3-architecture)
4. [Crate Structure](#4-crate-structure)
5. [Key Management](#5-key-management)
6. [DERP Integration](#6-derp-integration)
7. [Connection Flow](#7-connection-flow)
8. [Database Schema](#8-database-schema)
9. [API Endpoints](#9-api-endpoints)
10. [CLI Commands](#10-cli-commands)
11. [Weaver Integration](#11-weaver-integration)
12. [Security Considerations](#12-security-considerations)
13. [Implementation Phases](#13-implementation-phases)
14. [Configuration Reference](#14-configuration-reference)
15. [Testing Strategy](#15-testing-strategy)

---

## 1. Overview

### 1.1 Purpose

The WireGuard Tunnel System provides secure, direct network connectivity between authenticated users and their Loom weavers. It enables:

- **SSH Access**: Users can SSH directly into their weaver pods
- **TCP Forwarding**: Access web servers and other services running on weavers
- **NAT Traversal**: Works across firewalls and NATs using DERP relays
- **Multi-device Support**: Connect to the same weaver from multiple devices simultaneously

### 1.2 Design Principles

| Principle | Implementation |
|-----------|----------------|
| **P2P when possible** | Direct WireGuard UDP connections preferred |
| **Relay when needed** | DERP servers as encrypted fallback |
| **Zero server relay** | loom-server coordinates only, never sees traffic |
| **Ephemeral weaver keys** | New WireGuard keypair per weaver pod |
| **Persistent device keys** | User devices keep stable identity |
| **SPIFFE-based auth** | Weavers authenticate via existing SVID system |

### 1.3 Relationship to Existing Systems

| Existing System | Relationship |
|-----------------|--------------|
| `loom attach` (WebSocket) | Kept as alternative; WG tunnel is additive |
| `loom-weaver-secrets` | Reuse SVID authentication for WG registration |
| `loom-server-k8s` | Provisions weavers with WG daemon |
| `loom-server-jobs` | Cleans up WG registrations on weaver termination |

---

## 2. Goals and Non-Goals

### 2.1 Goals

- **Secure direct connectivity** between user devices and weavers via WireGuard
- **Works everywhere** - DERP relay fallback for restrictive networks
- **Low latency** - direct P2P when NAT traversal succeeds
- **Simple UX** - `loom ssh <weaver>` just works
- **Multi-device** - same weaver accessible from laptop, desktop, etc.
- **General TCP** - SSH, HTTP, database connections over tunnel

### 2.2 Non-Goals (v1)

- Weaver-to-weaver communication (explicitly isolated)
- Full mesh networking between all user devices
- UDP forwarding (TCP only for v1)
- Running Tailscale/Headscale as a product dependency
- Kernel WireGuard (userspace only for portability)

---

## 3. Architecture

### 3.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              User Device                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  loom-cli                                                            │    │
│  │  ├── WireGuard Engine (boringtun)                                    │    │
│  │  ├── DERP Client                                                     │    │
│  │  ├── MagicConn (direct UDP + DERP mux)                               │    │
│  │  └── Device WG Keypair (~/.loom/wg-key)                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────┬───────────────────────────────────────────┘
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
                    ▼                           ▼
    ┌───────────────────────────┐   ┌───────────────────────────────┐
    │   Direct UDP (preferred)   │   │   DERP Relay (fallback)       │
    │   - If NAT allows          │   │   - Tailscale public DERP     │
    │   - STUN discovery         │   │   - Custom DERP overlay       │
    └───────────────────────────┘   └───────────────────────────────┘
                    │                           │
                    └─────────────┬─────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Weaver Pod                                      │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  loom-weaver-wgtunnel daemon                                         │    │
│  │  ├── WireGuard Engine (boringtun)                                    │    │
│  │  ├── DERP Client                                                     │    │
│  │  ├── MagicConn                                                       │    │
│  │  ├── Ephemeral WG Keypair (in-memory)                                │    │
│  │  └── SVID Authentication                                             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                       │
│  │  SSH Daemon  │  │  Web Server  │  │  Other TCP   │                       │
│  │  :22         │  │  :3000       │  │  Services    │                       │
│  └──────────────┘  └──────────────┘  └──────────────┘                       │
└─────────────────────────────────────────────────────────────────────────────┘
                                  ▲
                                  │ Control Plane Only
                                  │ (key exchange, no traffic relay)
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  loom-server-wgtunnel                                                │    │
│  │  ├── Device Registration API                                         │    │
│  │  ├── Weaver WG Registration API (SVID auth)                          │    │
│  │  ├── Session Creation API                                            │    │
│  │  ├── Peer Streaming (notify weavers of new clients)                  │    │
│  │  ├── IP Allocator (fd7a:115c:a1e0::/48)                              │    │
│  │  └── DERP Map Provider                                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow: SSH Connection

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. User runs: loom ssh my-weaver                                             │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. loom-cli: Check local WG key exists                                       │
│    - If not: generate keypair, register with loom-server                     │
│    - POST /api/wg/devices {device_id, wg_public_key}                         │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. loom-cli: Request tunnel session                                          │
│    - POST /api/wg/sessions {weaver_id, device_id, client_wg_pubkey}          │
│    - Server validates: user owns weaver, device is registered                │
│    - Server allocates client_ip, notifies weaver of new peer                 │
│    - Returns: {weaver_wg_pubkey, weaver_ip, client_ip, derp_map}             │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. loom-cli: Configure WireGuard                                             │
│    - Create virtual interface (userspace TUN)                                │
│    - Set local address = client_ip                                           │
│    - Add peer: weaver_wg_pubkey, allowed_ips = weaver_ip/128                 │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. Connection Establishment                                                  │
│    a. Try direct UDP to weaver's discovered endpoint (STUN)                  │
│    b. If fails → connect via DERP relay                                      │
│    c. WireGuard handshake completes                                          │
│    d. Periodically retry direct connection (upgrade from DERP)               │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 6. SSH over WireGuard                                                        │
│    - loom-cli spawns: ssh user@{weaver_ip}                                   │
│    - Or: built-in SSH client connects to weaver_ip:22                        │
│    - Traffic flows directly (or via DERP), never through loom-server         │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Crate Structure

### 4.1 New Crates

```
crates/
├── loom-wgtunnel-common/          # Shared types and utilities
│   ├── src/
│   │   ├── lib.rs
│   │   ├── keys.rs                # WgPrivateKey (Secret<>), WgPublicKey, WgKeyPair
│   │   ├── keys_file.rs           # Key file loading with *_FILE convention
│   │   ├── ip.rs                  # IP allocation, subnet management
│   │   ├── derp_map.rs            # DERP map types, loading, overlay
│   │   ├── peer.rs                # PeerId, PeerInfo
│   │   └── session.rs             # SessionId, SessionInfo
│   └── Cargo.toml
│
├── loom-wgtunnel-derp/            # DERP protocol implementation
│   ├── src/
│   │   ├── lib.rs
│   │   ├── protocol.rs            # DERP frame types, encoding/decoding
│   │   ├── client.rs              # DERP client (connect to relay)
│   │   ├── server.rs              # DERP server (optional self-hosted)
│   │   └── map.rs                 # Fetch and merge DERP maps
│   └── Cargo.toml
│
├── loom-wgtunnel-conn/            # Smart connection multiplexer
│   ├── src/
│   │   ├── lib.rs
│   │   ├── magic_conn.rs          # MagicConn: direct + DERP mux
│   │   ├── stun.rs                # STUN client for NAT discovery
│   │   ├── endpoint.rs            # Endpoint discovery and selection
│   │   └── upgrade.rs             # DERP → direct upgrade logic
│   └── Cargo.toml
│
├── loom-wgtunnel-engine/          # WireGuard engine wrapper
│   ├── src/
│   │   ├── lib.rs
│   │   ├── engine.rs              # WgEngine: boringtun + MagicConn
│   │   ├── device.rs              # Virtual TUN device
│   │   ├── peers.rs               # Peer management
│   │   └── router.rs              # Packet routing
│   └── Cargo.toml
│
├── loom-server-wgtunnel/          # Server-side coordination
│   ├── src/
│   │   ├── lib.rs
│   │   ├── config.rs              # Server WG tunnel configuration
│   │   ├── devices.rs             # Device registration logic
│   │   ├── weavers.rs             # Weaver WG registration logic
│   │   ├── sessions.rs            # Session management
│   │   ├── ip_allocator.rs        # IP address allocation
│   │   ├── peer_notify.rs         # Notify weavers of peer changes
│   │   └── derp_map.rs            # DERP map serving with overlay
│   └── Cargo.toml
│
├── loom-cli-wgtunnel/             # CLI integration
│   ├── src/
│   │   ├── lib.rs
│   │   ├── tunnel.rs              # Tunnel up/down commands
│   │   ├── ssh.rs                 # SSH command (tunnel + ssh)
│   │   └── daemon.rs              # Background tunnel daemon
│   └── Cargo.toml
│
└── loom-weaver-wgtunnel/          # Weaver-side daemon
    ├── src/
    │   ├── lib.rs
    │   ├── daemon.rs              # Main daemon loop
    │   ├── registration.rs        # Register WG key with server
    │   ├── peer_stream.rs         # Stream peer updates from server
    │   └── svid.rs                # SVID-based authentication
    └── Cargo.toml
```

### 4.2 Dependency Graph

```
loom-common-secret ◄──────────────────────────────────────────┐
                                                              │
loom-wgtunnel-common ◄────────────────────────────────────────┤
        ▲                                                     │
        │                                                     │
        ├──────────────────┬──────────────────┐               │
        │                  │                  │               │
loom-wgtunnel-derp    loom-wgtunnel-conn    loom-wgtunnel-engine
        ▲                  ▲                  ▲               │
        │                  │                  │               │
        └──────────────────┴──────────────────┘               │
                           │                                  │
        ┌──────────────────┼──────────────────┐               │
        │                  │                  │               │
loom-server-wgtunnel  loom-cli-wgtunnel  loom-weaver-wgtunnel │
        │                  │                  │               │
        └──────────────────┴──────────────────┴───────────────┘
```

---

## 5. Key Management

### 5.1 Key Types

```rust
use loom_common_secret::{Secret, REDACTED};
use zeroize::Zeroize;

/// 32-byte WireGuard key (Curve25519)
#[derive(Clone, Zeroize)]
pub struct WgKeyBytes([u8; 32]);

/// Private key - wrapped in Secret for protection
pub type WgPrivateKey = Secret<WgKeyBytes>;

/// Public key - safe to share/log
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WgPublicKey([u8; 32]);

impl WgPublicKey {
    /// Base64 encoding for API transport
    pub fn to_base64(&self) -> String;
    
    /// Parse from base64
    pub fn from_base64(s: &str) -> Result<Self>;
}

/// Keypair with protected private key
pub struct WgKeyPair {
    pub private: WgPrivateKey,  // Zeroized on drop, redacted in logs
    pub public: WgPublicKey,    // Safe to log/serialize
}

impl WgKeyPair {
    /// Generate new random keypair
    pub fn generate() -> Self;
    
    /// Derive public key from private
    pub fn from_private(private: WgPrivateKey) -> Self;
}
```

### 5.2 Key Lifecycle

| Component | Key Type | Lifetime | Storage |
|-----------|----------|----------|---------|
| User device | Persistent | Until revoked | `~/.loom/wg-key` (0600 permissions) |
| Weaver pod | Ephemeral | Pod lifetime | In-memory, zeroized on exit |
| loom-server | None | N/A | Only stores public keys |

### 5.3 Device Key Storage

Following the `*_FILE` convention from `loom-common-config`:

```bash
# Direct (dev/testing)
export LOOM_WG_PRIVATE_KEY="base64-encoded-key"

# File-based (production)
export LOOM_WG_PRIVATE_KEY_FILE="~/.loom/wg-key"
```

```rust
use loom_common_config::load_secret_env;

pub fn load_or_generate_device_key() -> Result<WgKeyPair> {
    // Check LOOM_WG_PRIVATE_KEY_FILE first, then LOOM_WG_PRIVATE_KEY
    if let Some(key_b64) = load_secret_env("LOOM_WG_PRIVATE_KEY")? {
        let bytes = base64_decode(key_b64.expose())?;
        return WgKeyPair::from_private_bytes(bytes);
    }
    
    // Check default file location
    let key_path = dirs::config_dir()
        .ok_or_else(|| anyhow!("no config dir"))?
        .join("loom")
        .join("wg-key");
    
    if key_path.exists() {
        let key_b64 = std::fs::read_to_string(&key_path)?;
        let bytes = base64_decode(key_b64.trim())?;
        return WgKeyPair::from_private_bytes(bytes);
    }
    
    // Generate new keypair
    let keypair = WgKeyPair::generate();
    
    // Write with restricted permissions
    std::fs::create_dir_all(key_path.parent().unwrap())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&key_path)?;
        file.write_all(keypair.private.expose().to_base64().as_bytes())?;
    }
    
    Ok(keypair)
}
```

---

## 6. DERP Integration

### 6.1 DERP Protocol Overview

DERP (Designated Encrypted Relay for Packets) provides:
- **NAT traversal** when direct UDP fails
- **Encrypted relay** - WireGuard packets wrapped in HTTPS
- **Global distribution** - Tailscale operates ~30 regions worldwide

### 6.2 DERP Map Structure

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerpMap {
    /// Region ID → Region info
    #[serde(rename = "Regions")]
    pub regions: HashMap<u16, Option<DerpRegion>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerpRegion {
    #[serde(rename = "RegionID")]
    pub region_id: u16,
    
    #[serde(rename = "RegionCode")]
    pub region_code: String,  // e.g., "nyc", "sfo"
    
    #[serde(rename = "RegionName")]
    pub region_name: String,  // e.g., "New York City"
    
    #[serde(rename = "Latitude")]
    pub latitude: f64,
    
    #[serde(rename = "Longitude")]
    pub longitude: f64,
    
    #[serde(rename = "Nodes")]
    pub nodes: Vec<DerpNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerpNode {
    #[serde(rename = "Name")]
    pub name: String,  // e.g., "1f"
    
    #[serde(rename = "RegionID")]
    pub region_id: u16,
    
    #[serde(rename = "HostName")]
    pub hostname: String,  // e.g., "derp1f.tailscale.com"
    
    #[serde(rename = "IPv4")]
    pub ipv4: Option<String>,
    
    #[serde(rename = "IPv6")]
    pub ipv6: Option<String>,
    
    #[serde(rename = "DERPPort", default = "default_derp_port")]
    pub derp_port: u16,  // Usually 443
    
    #[serde(rename = "CanPort80", default)]
    pub can_port_80: bool,
}

fn default_derp_port() -> u16 { 443 }
```

### 6.3 DERP Map Loading

```rust
/// Load DERP map from multiple sources with overlay
pub async fn load_derp_map(config: &DerpConfig) -> Result<DerpMap> {
    // 1. Fetch Tailscale public DERP map
    let base_map = fetch_tailscale_derp_map().await?;
    
    // 2. Apply overlay configuration (disable regions, add custom)
    let map = apply_overlay(base_map, &config.overlay)?;
    
    Ok(map)
}

async fn fetch_tailscale_derp_map() -> Result<DerpMap> {
    let url = "https://controlplane.tailscale.com/derpmap/default";
    let resp = reqwest::get(url).await?;
    let map: DerpMap = resp.json().await?;
    Ok(map)
}

/// Overlay configuration (YAML format, like Headscale)
#[derive(Debug, Clone, Deserialize)]
pub struct DerpOverlay {
    /// Regions to disable (set to null)
    #[serde(default)]
    pub disable_regions: Vec<u16>,
    
    /// Custom regions to add
    #[serde(default)]
    pub custom_regions: HashMap<u16, DerpRegion>,
    
    /// Disable all Tailscale regions (only use custom)
    #[serde(default)]
    pub omit_default_regions: bool,
}
```

### 6.4 DERP Client Protocol

```rust
/// DERP frame types (matches Tailscale protocol)
#[derive(Debug)]
pub enum DerpFrame {
    /// Server → Client: Server info after connection
    ServerInfo { derp_pub_key: [u8; 32] },
    
    /// Client → Server: Client identifies itself
    ClientInfo { public_key: WgPublicKey },
    
    /// Bidirectional: Send packet to peer
    SendPacket { dst_key: WgPublicKey, data: Vec<u8> },
    
    /// Server → Client: Received packet from peer
    RecvPacket { src_key: WgPublicKey, data: Vec<u8> },
    
    /// Keepalive
    KeepAlive,
    
    /// Peer went away
    PeerGone { peer_key: WgPublicKey },
    
    /// Peer is present on this DERP
    PeerPresent { peer_key: WgPublicKey },
}

/// DERP client connection
pub struct DerpClient {
    stream: TlsStream<TcpStream>,
    our_key: WgKeyPair,
    server_key: [u8; 32],
    home_region: u16,
}

impl DerpClient {
    /// Connect to a DERP server
    pub async fn connect(
        node: &DerpNode,
        our_key: &WgKeyPair,
    ) -> Result<Self>;
    
    /// Send a packet to a peer via DERP
    pub async fn send(&mut self, dst: &WgPublicKey, data: &[u8]) -> Result<()>;
    
    /// Receive next packet
    pub async fn recv(&mut self) -> Result<(WgPublicKey, Vec<u8>)>;
}
```

---

## 7. Connection Flow

### 7.1 MagicConn Architecture

```rust
/// Magic connection that tries direct UDP, falls back to DERP
pub struct MagicConn {
    /// Our WireGuard keypair
    our_key: WgKeyPair,
    
    /// UDP socket for direct connections
    udp: UdpSocket,
    
    /// DERP client connections (region_id → client)
    derp_clients: HashMap<u16, DerpClient>,
    
    /// Home DERP region (lowest latency)
    home_derp: u16,
    
    /// Known peer endpoints
    peer_endpoints: HashMap<WgPublicKey, PeerEndpoint>,
    
    /// DERP map
    derp_map: DerpMap,
}

#[derive(Debug)]
pub struct PeerEndpoint {
    /// Direct UDP endpoint if known
    pub direct: Option<SocketAddr>,
    
    /// DERP region where peer is connected
    pub derp_region: Option<u16>,
    
    /// Last successful direct connection
    pub last_direct: Option<Instant>,
    
    /// Are we currently using DERP?
    pub using_derp: bool,
}

impl MagicConn {
    /// Send packet to peer, using best available path
    pub async fn send(&self, peer: &WgPublicKey, data: &[u8]) -> Result<()> {
        let endpoint = self.peer_endpoints.get(peer);
        
        // Try direct first if we have an endpoint and it worked recently
        if let Some(ep) = endpoint {
            if let Some(direct) = ep.direct {
                if !ep.using_derp || ep.last_direct.map_or(false, |t| t.elapsed() < DIRECT_RETRY) {
                    if self.udp.send_to(data, direct).await.is_ok() {
                        return Ok(());
                    }
                }
            }
        }
        
        // Fall back to DERP
        if let Some(ep) = endpoint {
            if let Some(region) = ep.derp_region {
                if let Some(client) = self.derp_clients.get_mut(&region) {
                    return client.send(peer, data).await;
                }
            }
        }
        
        // Try home DERP as last resort
        if let Some(client) = self.derp_clients.get_mut(&self.home_derp) {
            return client.send(peer, data).await;
        }
        
        Err(anyhow!("no path to peer"))
    }
    
    /// Receive next packet from any source
    pub async fn recv(&self) -> Result<(WgPublicKey, Vec<u8>, PathType)> {
        tokio::select! {
            // Direct UDP
            result = self.udp.recv_from(&mut buf) => {
                let (len, addr) = result?;
                // Decode WireGuard to get peer key
                let peer = extract_peer_from_wg_packet(&buf[..len])?;
                Ok((peer, buf[..len].to_vec(), PathType::Direct))
            }
            
            // DERP (poll all connections)
            result = self.recv_from_any_derp() => {
                let (peer, data, region) = result?;
                Ok((peer, data, PathType::Derp(region)))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PathType {
    Direct,
    Derp(u16),
}
```

### 7.2 STUN Discovery

```rust
/// Discover our public endpoint via STUN
pub async fn discover_endpoint(stun_servers: &[SocketAddr]) -> Result<SocketAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    
    for server in stun_servers {
        let binding = stun_binding_request(&socket, *server).await;
        if let Ok(addr) = binding {
            return Ok(addr);
        }
    }
    
    Err(anyhow!("STUN discovery failed"))
}

/// STUN servers to use (Tailscale's, Google's, etc.)
pub const DEFAULT_STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302",
    "stun.cloudflare.com:3478",
];
```

### 7.3 Path Upgrade (DERP → Direct)

```rust
impl MagicConn {
    /// Background task to upgrade DERP connections to direct
    async fn upgrade_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            for (peer, endpoint) in &self.peer_endpoints {
                if endpoint.using_derp {
                    // Try to discover direct path
                    if let Some(direct) = self.probe_direct(peer).await {
                        // Update endpoint
                        endpoint.direct = Some(direct);
                        endpoint.using_derp = false;
                        endpoint.last_direct = Some(Instant::now());
                        
                        tracing::info!(
                            peer = %peer,
                            endpoint = %direct,
                            "Upgraded from DERP to direct"
                        );
                    }
                }
            }
        }
    }
}
```

---

## 8. Database Schema

### 8.1 Tables

```sql
-- User's registered WireGuard devices (persistent keys)
CREATE TABLE wg_devices (
    id TEXT PRIMARY KEY,                    -- UUID
    user_id TEXT NOT NULL REFERENCES users(id),
    public_key BLOB NOT NULL UNIQUE,        -- 32 bytes, WireGuard public key
    name TEXT,                              -- User-friendly name: "MacBook Pro"
    created_at TEXT NOT NULL,               -- ISO 8601
    last_seen_at TEXT,                      -- Last API activity
    revoked_at TEXT,                        -- Null if active
    
    UNIQUE(user_id, public_key)
);

CREATE INDEX idx_wg_devices_user ON wg_devices(user_id);
CREATE INDEX idx_wg_devices_pubkey ON wg_devices(public_key);

-- Active weaver WireGuard registrations (ephemeral, per-pod)
CREATE TABLE wg_weavers (
    weaver_id TEXT PRIMARY KEY REFERENCES weavers(id),
    public_key BLOB NOT NULL,               -- 32 bytes, ephemeral WG key
    assigned_ip TEXT NOT NULL,              -- fd7a:115c:a1e0::xxxx
    derp_home_region INTEGER,               -- Home DERP region ID
    endpoint TEXT,                          -- Direct UDP endpoint if known
    registered_at TEXT NOT NULL,
    last_seen_at TEXT
);

CREATE INDEX idx_wg_weavers_pubkey ON wg_weavers(public_key);

-- Active tunnel sessions (device ↔ weaver connections)
CREATE TABLE wg_sessions (
    id TEXT PRIMARY KEY,                    -- UUID
    device_id TEXT NOT NULL REFERENCES wg_devices(id),
    weaver_id TEXT NOT NULL REFERENCES wg_weavers(weaver_id),
    client_ip TEXT NOT NULL,                -- fd7a:115c:a1e0::yyyy
    created_at TEXT NOT NULL,
    last_handshake_at TEXT,                 -- Last WireGuard handshake
    
    UNIQUE(device_id, weaver_id)
);

CREATE INDEX idx_wg_sessions_device ON wg_sessions(device_id);
CREATE INDEX idx_wg_sessions_weaver ON wg_sessions(weaver_id);

-- IP allocation tracking
CREATE TABLE wg_ip_allocations (
    ip TEXT PRIMARY KEY,                    -- fd7a:115c:a1e0::xxxx
    allocation_type TEXT NOT NULL,          -- 'weaver' or 'client'
    entity_id TEXT NOT NULL,                -- weaver_id or session_id
    allocated_at TEXT NOT NULL,
    released_at TEXT                        -- Null if still allocated
);

CREATE INDEX idx_wg_ip_alloc_entity ON wg_ip_allocations(entity_id);
```

### 8.2 IP Allocation

```rust
/// IP allocator for WireGuard overlay network
pub struct IpAllocator {
    /// Base prefix: fd7a:115c:a1e0::/48 (Tailscale's ULA)
    prefix: Ipv6Net,
    
    /// Database connection
    db: SqlitePool,
}

impl IpAllocator {
    /// Allocate IP for a weaver
    pub async fn allocate_weaver(&self, weaver_id: &str) -> Result<Ipv6Addr> {
        // Weavers get IPs in fd7a:115c:a1e0:1::/64
        self.allocate("weaver", weaver_id, 1).await
    }
    
    /// Allocate IP for a client session
    pub async fn allocate_client(&self, session_id: &str) -> Result<Ipv6Addr> {
        // Clients get IPs in fd7a:115c:a1e0:2::/64
        self.allocate("client", session_id, 2).await
    }
    
    /// Release an IP allocation
    pub async fn release(&self, ip: Ipv6Addr) -> Result<()>;
}
```

---

## 9. API Endpoints

### 9.1 Device Registration

**POST /api/wg/devices**

Register a new WireGuard device for the authenticated user.

```typescript
// Request
{
    "device_id": "550e8400-e29b-41d4-a716-446655440000",  // Client-generated UUID
    "public_key": "base64-encoded-32-byte-key",
    "name": "MacBook Pro"  // Optional
}

// Response 201 Created
{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "public_key": "base64-encoded-32-byte-key",
    "name": "MacBook Pro",
    "created_at": "2026-01-03T12:00:00Z"
}
```

**GET /api/wg/devices**

List user's registered devices.

**DELETE /api/wg/devices/{device_id}**

Revoke a device (marks as revoked, terminates active sessions).

### 9.2 Weaver WG Registration (Internal)

**POST /internal/wg/weavers/register**

Called by weaver on startup. Authenticated via SVID.

```typescript
// Request (Authorization: Bearer <weaver-svid>)
{
    "weaver_id": "weaver-abc123",
    "public_key": "base64-encoded-32-byte-key",
    "derp_home_region": 1  // NYC
}

// Response 200 OK
{
    "assigned_ip": "fd7a:115c:a1e0:1::abc",
    "derp_map_url": "https://loom.example.com/api/wg/derp-map",
    "peers_stream_url": "wss://loom.example.com/internal/wg/weavers/weaver-abc123/peers"
}
```

**WebSocket /internal/wg/weavers/{weaver_id}/peers**

Stream of peer updates for the weaver.

```typescript
// Server → Weaver: New peer connected
{
    "action": "add",
    "peer": {
        "public_key": "base64-encoded-32-byte-key",
        "allowed_ip": "fd7a:115c:a1e0:2::xyz/128"
    }
}

// Server → Weaver: Peer disconnected
{
    "action": "remove",
    "public_key": "base64-encoded-32-byte-key"
}
```

### 9.3 Session Management

**POST /api/wg/sessions**

Create a tunnel session to a weaver.

```typescript
// Request
{
    "weaver_id": "weaver-abc123",
    "device_id": "550e8400-e29b-41d4-a716-446655440000"
}

// Response 201 Created
{
    "session_id": "sess-xyz789",
    "client_ip": "fd7a:115c:a1e0:2::xyz",
    "weaver": {
        "public_key": "base64-encoded-32-byte-key",
        "ip": "fd7a:115c:a1e0:1::abc",
        "derp_home_region": 1
    },
    "derp_map": { /* full DERP map */ }
}
```

**DELETE /api/wg/sessions/{session_id}**

Terminate a session.

**GET /api/wg/sessions**

List active sessions for user.

### 9.4 DERP Map

**GET /api/wg/derp-map**

Get DERP map with any configured overlays.

```typescript
// Response
{
    "Regions": {
        "1": {
            "RegionID": 1,
            "RegionCode": "nyc",
            "RegionName": "New York City",
            "Nodes": [...]
        },
        // ... more regions
    }
}
```

---

## 10. CLI Commands

### 10.1 Tunnel Commands

```bash
# Start tunnel to weaver (daemon mode)
loom tunnel up <weaver-id>
  --background              # Run as background daemon
  --interface <name>        # TUN interface name (default: loom0)

# Stop tunnel
loom tunnel down <weaver-id>

# List active tunnels
loom tunnel list

# Show tunnel status
loom tunnel status <weaver-id>
```

### 10.2 SSH Command

```bash
# SSH to weaver (combines tunnel + ssh)
loom ssh <weaver-id>
  --port <port>             # SSH port (default: 22)
  --user <user>             # SSH user (default: loom)
  --identity <path>         # SSH identity file
  --forward <local:remote>  # Port forwarding (-L equivalent)

# Examples:
loom ssh my-weaver
loom ssh my-weaver --forward 3000:localhost:3000
loom ssh my-weaver -- -L 5432:localhost:5432  # Pass args to ssh
```

### 10.3 Device Management

```bash
# List registered devices
loom wg devices list

# Register current device (usually automatic)
loom wg devices register
  --name "MacBook Pro"

# Revoke a device
loom wg devices revoke <device-id>

# Rotate device key
loom wg devices rotate
```

### 10.4 Implementation

```rust
// loom-cli-wgtunnel/src/ssh.rs

pub async fn run_ssh(
    server_url: &Url,
    token: &SecretString,
    weaver_id: &str,
    ssh_args: SshArgs,
) -> Result<()> {
    // 1. Ensure device is registered
    let device_key = load_or_generate_device_key()?;
    let device_id = ensure_device_registered(server_url, token, &device_key).await?;
    
    // 2. Create tunnel session
    let session = create_session(server_url, token, weaver_id, &device_id).await?;
    
    // 3. Set up WireGuard
    let engine = WgEngine::new(device_key)?;
    engine.add_peer(&session.weaver.public_key, &session.weaver.ip)?;
    engine.set_address(&session.client_ip)?;
    
    // 4. Start MagicConn (DERP + direct)
    let conn = MagicConn::new(&session.derp_map, &device_key)?;
    engine.set_conn(conn);
    engine.up().await?;
    
    // 5. Wait for WireGuard handshake
    engine.wait_handshake(&session.weaver.public_key, Duration::from_secs(10)).await?;
    
    // 6. Spawn SSH
    let ssh_target = format!("{}@{}", ssh_args.user, session.weaver.ip);
    let status = Command::new("ssh")
        .arg(&ssh_target)
        .args(&ssh_args.extra_args)
        .status()
        .await?;
    
    // 7. Cleanup
    engine.down().await?;
    delete_session(server_url, token, &session.session_id).await?;
    
    std::process::exit(status.code().unwrap_or(1));
}
```

---

## 11. Weaver Integration

### 11.1 Weaver Image Changes

Add to weaver container image:

```dockerfile
# Install OpenSSH server
RUN apt-get update && apt-get install -y openssh-server

# Configure SSH
RUN mkdir /var/run/sshd
RUN echo 'PermitRootLogin no' >> /etc/ssh/sshd_config
RUN echo 'PasswordAuthentication no' >> /etc/ssh/sshd_config
RUN echo 'PubkeyAuthentication yes' >> /etc/ssh/sshd_config

# Create loom user
RUN useradd -m -s /bin/bash loom
RUN mkdir -p /home/loom/.ssh

# WireGuard daemon will be started by weaver init
```

### 11.2 Weaver WG Daemon

```rust
// loom-weaver-wgtunnel/src/daemon.rs

pub async fn run_wg_daemon() -> Result<()> {
    // 1. Get SVID for authentication
    let svid_client = SvidClient::new()?;
    let svid = svid_client.get_svid().await?;
    
    // 2. Generate ephemeral WireGuard keypair
    let keypair = WgKeyPair::generate();
    tracing::info!(public_key = %keypair.public, "Generated WireGuard keypair");
    
    // 3. Register with loom-server
    let registration = register_with_server(&svid, &keypair).await?;
    tracing::info!(
        assigned_ip = %registration.assigned_ip,
        "Registered with coordination server"
    );
    
    // 4. Set up WireGuard engine
    let engine = WgEngine::new(keypair)?;
    engine.set_address(&registration.assigned_ip)?;
    
    // 5. Connect to DERP
    let derp_map = fetch_derp_map(&registration.derp_map_url).await?;
    let conn = MagicConn::new(&derp_map, &keypair)?;
    engine.set_conn(conn);
    engine.up().await?;
    
    // 6. Stream peer updates
    let peers_stream = connect_peers_stream(&registration.peers_stream_url, &svid).await?;
    
    tokio::spawn(async move {
        while let Some(update) = peers_stream.next().await {
            match update {
                PeerUpdate::Add { public_key, allowed_ip } => {
                    engine.add_peer(&public_key, &allowed_ip)?;
                    tracing::info!(peer = %public_key, "Added peer");
                }
                PeerUpdate::Remove { public_key } => {
                    engine.remove_peer(&public_key)?;
                    tracing::info!(peer = %public_key, "Removed peer");
                }
            }
        }
    });
    
    // 7. Run until shutdown signal
    shutdown_signal().await;
    
    // 8. Cleanup
    engine.down().await?;
    
    Ok(())
}
```

### 11.3 Weaver Startup Sequence

```
Pod Start
    │
    ├── 1. Init containers (if any)
    │
    ├── 2. Main container starts
    │       ├── loom-weaver-wgtunnel daemon (background)
    │       ├── sshd (background)
    │       └── Main weaver process
    │
    └── 3. WG daemon:
            ├── Get SVID from loom-server
            ├── Generate ephemeral WG key
            ├── Register with /internal/wg/weavers/register
            ├── Configure WG interface
            └── Stream /internal/wg/weavers/{id}/peers
```

---

## 12. Security Considerations

### 12.1 Threat Model

| Threat | Mitigation |
|--------|------------|
| Impersonating a weaver | SVID authentication required for registration |
| Impersonating a user device | Loom auth token required for session creation |
| Traffic interception | WireGuard encryption (ChaCha20-Poly1305) |
| DERP server sees traffic | WG packets are encrypted before DERP relay |
| Weaver-to-weaver attacks | AllowedIPs restrict traffic to assigned client only |
| Stolen device key | Revocation via API, short session TTL |
| Server compromise | Server never sees private keys, only public |

### 12.2 Key Security

```rust
// Keys are wrapped in Secret<T> - auto-redacted in logs
let keypair = WgKeyPair::generate();

// ✅ Safe - public key
tracing::info!(public_key = %keypair.public, "Generated keypair");

// ✅ Safe - private key redacted
tracing::debug!(?keypair.private, "Private key loaded");  // Shows "[REDACTED]"

// ❌ Never do this
tracing::info!(key = %keypair.private.expose(), "LEAKS!");
```

### 12.3 Network Isolation

```
Weaver A (fd7a:115c:a1e0:1::a)
    AllowedIPs: fd7a:115c:a1e0:2::1/128  (User's device only)
    
Weaver B (fd7a:115c:a1e0:1::b)
    AllowedIPs: fd7a:115c:a1e0:2::2/128  (Different user's device only)
    
→ Weaver A cannot send traffic to Weaver B (not in AllowedIPs)
→ Weaver A cannot receive traffic from Weaver B (WG drops it)
```

### 12.4 Capabilities Required

```yaml
# Weaver pod security context
securityContext:
  capabilities:
    add:
      - NET_ADMIN  # Required for TUN interface
    drop:
      - ALL
```

---

## 13. Implementation Phases

### Phase 1: Foundation (Week 1)

**Goal:** Core types and basic WireGuard engine

- [ ] Create `loom-wgtunnel-common` crate
  - [ ] `WgKeyPair`, `WgPublicKey`, `WgPrivateKey` types
  - [ ] Key file loading with `*_FILE` convention
  - [ ] DERP map types
  - [ ] IP allocation types

- [ ] Create `loom-wgtunnel-engine` crate
  - [ ] Integrate `boringtun` for WireGuard
  - [ ] Basic peer add/remove
  - [ ] Packet send/recv

- [ ] Unit tests for key handling, serialization

**Milestone:** Can create WG engine, add peers, send encrypted packets

### Phase 2: DERP Client (Week 2)

**Goal:** Connect to Tailscale DERP servers

- [ ] Create `loom-wgtunnel-derp` crate
  - [ ] DERP frame encoding/decoding
  - [ ] TLS connection to DERP server
  - [ ] Client handshake (ServerInfo, ClientInfo)
  - [ ] SendPacket/RecvPacket

- [ ] Fetch and parse Tailscale DERP map
- [ ] Select home DERP based on latency

**Milestone:** Can relay WG packets through DERP

### Phase 3: MagicConn (Week 3)

**Goal:** Smart connection with direct + DERP

- [ ] Create `loom-wgtunnel-conn` crate
  - [ ] MagicConn: mux direct UDP and DERP
  - [ ] STUN client for endpoint discovery
  - [ ] Path selection logic
  - [ ] DERP → direct upgrade

- [ ] Integration with WG engine

**Milestone:** Two peers can connect across NATs via DERP, upgrade to direct

### Phase 4: Server APIs (Week 4)

**Goal:** Coordination server implementation

- [ ] Create `loom-server-wgtunnel` crate
  - [ ] Device registration API
  - [ ] Weaver WG registration API (SVID auth)
  - [ ] Session management API
  - [ ] Peer notification WebSocket
  - [ ] IP allocator

- [ ] Database migrations
- [ ] DERP map endpoint with overlay support

**Milestone:** loom-server can coordinate WG connections

### Phase 5: CLI Integration (Week 5)

**Goal:** User-facing tunnel commands

- [ ] Create `loom-cli-wgtunnel` crate
  - [ ] `loom tunnel up/down` commands
  - [ ] `loom ssh` command
  - [ ] Device registration flow
  - [ ] Background daemon mode

- [ ] Integrate with existing `loom-cli`

**Milestone:** `loom ssh my-weaver` works end-to-end

### Phase 6: Weaver Integration (Week 6)

**Goal:** Weaver-side WG daemon

- [ ] Create `loom-weaver-wgtunnel` crate
  - [ ] WG daemon main loop
  - [ ] SVID-based registration
  - [ ] Peer stream handling

- [ ] Add SSH daemon to weaver image
- [ ] Update weaver provisioner

**Milestone:** Full end-to-end: user → tunnel → weaver SSH

### Phase 7: Production Hardening (Week 7-8)

- [ ] DERP overlay configuration
- [ ] Optional self-hosted DERP server
- [ ] Metrics and observability
- [ ] Connection quality metrics
- [ ] Key rotation flows
- [ ] Session cleanup jobs
- [ ] Load testing
- [ ] Documentation

---

## 14. Configuration Reference

### 14.1 Environment Variables

#### loom-cli

| Variable | Required | Description |
|----------|----------|-------------|
| `LOOM_WG_PRIVATE_KEY` | No* | Base64-encoded WireGuard private key |
| `LOOM_WG_PRIVATE_KEY_FILE` | No* | Path to private key file |

*If neither set, key is auto-generated and stored in `~/.loom/wg-key`

#### loom-server

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LOOM_WG_ENABLED` | No | `true` | Enable WireGuard tunnel feature |
| `LOOM_WG_IP_PREFIX` | No | `fd7a:115c:a1e0::/48` | IP prefix for WG overlay |
| `LOOM_WG_DERP_MAP_URL` | No | Tailscale default | Base DERP map URL |
| `LOOM_WG_DERP_OVERLAY_FILE` | No | None | Path to DERP overlay YAML |

#### loom-weaver

| Variable | Required | Description |
|----------|----------|-------------|
| `LOOM_WG_ENABLED` | No | Enable WG daemon in weaver |
| `LOOM_SERVER_URL` | Yes | Coordination server URL |

### 14.2 DERP Overlay Configuration

```yaml
# /etc/loom/derp-overlay.yaml

# Disable specific Tailscale regions
disable_regions:
  - 6   # Bangalore (if latency is bad)

# Add custom DERP server
custom_regions:
  900:
    RegionID: 900
    RegionCode: "loom"
    RegionName: "Loom Private"
    Latitude: 37.7749
    Longitude: -122.4194
    Nodes:
      - Name: "loom1"
        RegionID: 900
        HostName: "derp.loom.example.com"
        IPv4: "203.0.113.50"
        DERPPort: 443

# Use only custom regions (disable all Tailscale)
omit_default_regions: false
```

### 14.3 NixOS Module

```nix
services.loom-server.wgtunnel = {
  enable = true;
  ipPrefix = "fd7a:115c:a1e0::/48";
  derpOverlayFile = "/etc/loom/derp-overlay.yaml";
};
```

---

## 15. Testing Strategy

### 15.1 Unit Tests

- Key generation, serialization, deserialization
- DERP frame encoding/decoding
- IP allocation logic
- API request/response serialization

### 15.2 Integration Tests

```rust
#[tokio::test]
async fn test_tunnel_session_flow() {
    let server = TestServer::start().await;
    
    // Register device
    let device = register_device(&server, &keypair).await;
    
    // Create weaver
    let weaver = create_weaver(&server).await;
    
    // Create session
    let session = create_session(&server, &device, &weaver).await;
    
    // Verify weaver received peer notification
    assert!(weaver.has_peer(&device.public_key));
    
    // Cleanup
    delete_session(&server, &session.id).await;
    assert!(!weaver.has_peer(&device.public_key));
}
```

### 15.3 End-to-End Tests

```rust
#[tokio::test]
async fn test_ssh_through_tunnel() {
    let server = TestServer::start().await;
    let weaver = TestWeaver::start(&server).await;
    
    // Run loom ssh command
    let output = Command::new("cargo")
        .args(["run", "-p", "loom-cli", "--", "ssh", &weaver.id])
        .arg("--")
        .arg("echo")
        .arg("hello")
        .output()
        .await?;
    
    assert_eq!(output.stdout, b"hello\n");
}
```

### 15.4 NAT Traversal Tests

- Local network (no NAT)
- Behind NAT with STUN
- Symmetric NAT (DERP only)
- DERP → direct upgrade

### 15.5 Load Tests

- 100+ concurrent sessions
- Sustained throughput measurement
- DERP server failover

---

## Appendix A: Rust Dependencies

```toml
[workspace.dependencies]
# WireGuard
boringtun = "0.6"

# Networking
smoltcp = { version = "0.11", features = ["medium-ip", "proto-ipv6", "socket-tcp"] }
tokio-tun = "0.11"

# DERP / TLS
tokio-rustls = "0.26"
rustls = "0.23"
webpki-roots = "0.26"

# Crypto
x25519-dalek = "2.0"
rand = "0.8"
base64 = "0.22"

# HTTP client
reqwest = { version = "0.12", features = ["rustls-tls", "json"] }

# WebSocket
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }

# Existing loom crates
loom-common-secret = { path = "crates/loom-common-secret" }
loom-common-config = { path = "crates/loom-common-config" }
loom-weaver-secrets = { path = "crates/loom-weaver-secrets" }
```

---

## Appendix B: DERP Protocol Reference

Based on Tailscale's BSD-licensed implementation.

### Frame Format

```
+--------+--------+--------+--------+
|  Type  |    Length (3 bytes)      |
+--------+--------+--------+--------+
|            Payload ...            |
+--------+--------+--------+--------+
```

### Frame Types

| Type | Name | Direction | Payload |
|------|------|-----------|---------|
| 0x01 | ServerKey | S→C | Server's public key (32 bytes) |
| 0x02 | ServerInfo | S→C | JSON with server info |
| 0x03 | SendPacket | C→S | dst_key (32) + data |
| 0x04 | RecvPacket | S→C | src_key (32) + data |
| 0x05 | KeepAlive | Both | Empty |
| 0x06 | NotePreferred | C→S | bool (1 byte) |
| 0x07 | PeerGone | S→C | peer_key (32) |
| 0x08 | PeerPresent | S→C | peer_key (32) |
| 0x09 | WatchConns | C→S | Empty (request notifications) |
| 0x0a | ClosePeer | C→S | peer_key (32) |

### Connection Handshake

1. Client connects via HTTPS to `/derp`
2. Upgrade to raw TCP (after TLS)
3. Server sends `ServerKey` + `ServerInfo`
4. Client sends `ClientInfo` with its public key
5. Bidirectional packet relay begins

---

## Appendix C: Related Specifications

| Document | Description |
|----------|-------------|
| [weaver-secrets-system.md](weaver-secrets-system.md) | SVID authentication used for weaver WG registration |
| [secret-system.md](secret-system.md) | `Secret<T>` wrapper for WireGuard private keys |
| [weaver-provisioner.md](weaver-provisioner.md) | K8s pod provisioning for weavers |
| [job-scheduler-system.md](job-scheduler-system.md) | Background jobs for session cleanup |

---

*This specification enables secure, direct network access to Loom weavers using WireGuard with DERP relay fallback.*
