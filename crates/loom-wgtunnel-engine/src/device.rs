// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{EngineError, Result};
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::tcp::{Socket as TcpSocket, SocketBuffer, State as TcpState};
use smoltcp::time::Instant as SmoltcpInstant;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr, Ipv6Address};
use std::collections::VecDeque;
use std::io;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Instant as StdInstant;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tracing::{debug, instrument, trace, warn};

const DEFAULT_TCP_RX_BUFFER_SIZE: usize = 65536;
const DEFAULT_TCP_TX_BUFFER_SIZE: usize = 65536;

/// Maximum number of packets in rx/tx queues to prevent memory exhaustion DoS
const MAX_QUEUE_SIZE: usize = 1024;

fn smoltcp_now() -> SmoltcpInstant {
	static START: std::sync::OnceLock<StdInstant> = std::sync::OnceLock::new();
	let start = START.get_or_init(StdInstant::now);
	SmoltcpInstant::from_micros(start.elapsed().as_micros() as i64)
}

struct InternalDevice {
	rx_queue: VecDeque<Vec<u8>>,
	tx_queue: VecDeque<Vec<u8>>,
	mtu: usize,
}

impl InternalDevice {
	fn new(mtu: u16) -> Self {
		Self {
			rx_queue: VecDeque::new(),
			tx_queue: VecDeque::new(),
			mtu: mtu as usize,
		}
	}
}

struct InternalRxToken {
	data: Vec<u8>,
}

impl RxToken for InternalRxToken {
	fn consume<R, F>(mut self, f: F) -> R
	where
		F: FnOnce(&mut [u8]) -> R,
	{
		f(&mut self.data)
	}
}

struct InternalTxToken<'a> {
	tx_queue: &'a mut VecDeque<Vec<u8>>,
}

impl<'a> TxToken for InternalTxToken<'a> {
	fn consume<R, F>(self, len: usize, f: F) -> R
	where
		F: FnOnce(&mut [u8]) -> R,
	{
		let mut buffer = vec![0u8; len];
		let result = f(&mut buffer);
		if self.tx_queue.len() >= MAX_QUEUE_SIZE {
			warn!(queue = "tx", "packet queue full, dropping oldest packet");
			self.tx_queue.pop_front();
		}
		self.tx_queue.push_back(buffer);
		result
	}
}

impl Device for InternalDevice {
	type RxToken<'a> = InternalRxToken;
	type TxToken<'a> = InternalTxToken<'a>;

	fn receive(
		&mut self,
		_timestamp: SmoltcpInstant,
	) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
		if let Some(data) = self.rx_queue.pop_front() {
			Some((
				InternalRxToken { data },
				InternalTxToken {
					tx_queue: &mut self.tx_queue,
				},
			))
		} else {
			None
		}
	}

	fn transmit(&mut self, _timestamp: SmoltcpInstant) -> Option<Self::TxToken<'_>> {
		Some(InternalTxToken {
			tx_queue: &mut self.tx_queue,
		})
	}

	fn capabilities(&self) -> DeviceCapabilities {
		let mut caps = DeviceCapabilities::default();
		caps.max_transmission_unit = self.mtu;
		caps.medium = Medium::Ip;
		caps
	}
}

struct DeviceInner {
	device: InternalDevice,
	iface: Interface,
	sockets: SocketSet<'static>,
	wakers: Vec<Waker>,
}

pub struct VirtualDevice {
	address: Ipv6Addr,
	mtu: u16,
	inner: Arc<Mutex<DeviceInner>>,
}

impl VirtualDevice {
	#[instrument(skip_all, fields(%address, mtu))]
	pub fn new(address: Ipv6Addr, mtu: u16) -> Result<Self> {
		let mut device = InternalDevice::new(mtu);

		let config = Config::new(HardwareAddress::Ip);
		let mut iface = Interface::new(config, &mut device, smoltcp_now());

		let smoltcp_addr = Ipv6Address::from_bytes(&address.octets());
		iface.update_ip_addrs(|addrs| {
			addrs
				.push(IpCidr::new(IpAddress::Ipv6(smoltcp_addr), 128))
				.ok();
		});

		let sockets = SocketSet::new(vec![]);

		debug!("created virtual device");

		Ok(Self {
			address,
			mtu,
			inner: Arc::new(Mutex::new(DeviceInner {
				device,
				iface,
				sockets,
				wakers: Vec::new(),
			})),
		})
	}

	#[instrument(skip(self, data), fields(len = data.len()))]
	pub fn receive_packet(&self, data: &[u8]) -> Result<()> {
		let mut inner = self
			.inner
			.lock()
			.map_err(|e| EngineError::Device(format!("lock poisoned: {}", e)))?;

		if inner.device.rx_queue.len() >= MAX_QUEUE_SIZE {
			warn!(queue = "rx", "packet queue full, dropping oldest packet");
			inner.device.rx_queue.pop_front();
		}
		inner.device.rx_queue.push_back(data.to_vec());

		self.poll_iface(&mut inner);

		for waker in inner.wakers.drain(..) {
			waker.wake();
		}

		trace!("received packet into virtual device");
		Ok(())
	}

	pub fn transmit_packet(&self) -> Option<Vec<u8>> {
		let mut inner = self.inner.lock().ok()?;

		self.poll_iface(&mut inner);

		let packet = inner.device.tx_queue.pop_front();
		if packet.is_some() {
			trace!("transmitting packet from virtual device");
		}
		packet
	}

	pub fn poll(&self) -> bool {
		let mut inner = match self.inner.lock() {
			Ok(i) => i,
			Err(_) => return false,
		};

		self.poll_iface(&mut inner)
	}

	fn poll_iface(&self, inner: &mut DeviceInner) -> bool {
		let timestamp = smoltcp_now();
		inner
			.iface
			.poll(timestamp, &mut inner.device, &mut inner.sockets)
	}

	pub fn address(&self) -> Ipv6Addr {
		self.address
	}

	pub fn mtu(&self) -> u16 {
		self.mtu
	}

	fn create_tcp_socket(&self) -> TcpSocket<'static> {
		let rx_buffer = SocketBuffer::new(vec![0u8; DEFAULT_TCP_RX_BUFFER_SIZE]);
		let tx_buffer = SocketBuffer::new(vec![0u8; DEFAULT_TCP_TX_BUFFER_SIZE]);
		TcpSocket::new(rx_buffer, tx_buffer)
	}

	pub fn listen(&self, port: u16) -> Result<(SocketHandle, SocketAddrV6)> {
		let mut inner = self
			.inner
			.lock()
			.map_err(|e| EngineError::Device(format!("lock poisoned: {}", e)))?;

		let mut socket = self.create_tcp_socket();
		socket
			.listen(port)
			.map_err(|e| EngineError::Device(format!("listen failed: {}", e)))?;

		let handle = inner.sockets.add(socket);
		let local_addr = SocketAddrV6::new(self.address, port, 0, 0);

		debug!(%port, "listening on port");
		Ok((handle, local_addr))
	}

	pub fn connect(&self, addr: SocketAddrV6) -> Result<SocketHandle> {
		let mut inner = self
			.inner
			.lock()
			.map_err(|e| EngineError::Device(format!("lock poisoned: {}", e)))?;

		let mut socket = self.create_tcp_socket();

		let local_port = 49152 + (fastrand::u16(..) % 16383);
		let local_endpoint = smoltcp::wire::IpEndpoint::new(
			IpAddress::Ipv6(Ipv6Address::from_bytes(&self.address.octets())),
			local_port,
		);
		let remote_endpoint = smoltcp::wire::IpEndpoint::new(
			IpAddress::Ipv6(Ipv6Address::from_bytes(&addr.ip().octets())),
			addr.port(),
		);

		socket
			.connect(inner.iface.context(), remote_endpoint, local_endpoint)
			.map_err(|e| EngineError::TcpConnect(format!("connect failed: {}", e)))?;

		let handle = inner.sockets.add(socket);

		debug!(%addr, "connecting to remote");
		Ok(handle)
	}

	pub fn socket_state(&self, handle: SocketHandle) -> Option<TcpState> {
		let inner = self.inner.lock().ok()?;
		let socket = inner.sockets.get::<TcpSocket>(handle);
		Some(socket.state())
	}

	pub fn register_waker(&self, waker: Waker) {
		if let Ok(mut inner) = self.inner.lock() {
			inner.wakers.push(waker);
		}
	}
}

impl Clone for VirtualDevice {
	fn clone(&self) -> Self {
		Self {
			address: self.address,
			mtu: self.mtu,
			inner: Arc::clone(&self.inner),
		}
	}
}

pub struct VirtualTcpListener {
	device: VirtualDevice,
	handle: SocketHandle,
	local_addr: SocketAddrV6,
}

impl VirtualTcpListener {
	pub(crate) fn new(device: VirtualDevice, handle: SocketHandle, local_addr: SocketAddrV6) -> Self {
		Self {
			device,
			handle,
			local_addr,
		}
	}

	pub async fn accept(&self) -> Result<(VirtualTcpStream, SocketAddrV6)> {
		loop {
			{
				let inner = self
					.device
					.inner
					.lock()
					.map_err(|e| EngineError::Device(format!("lock poisoned: {}", e)))?;

				let socket = inner.sockets.get::<TcpSocket>(self.handle);
				if socket.state() == TcpState::Established {
					if let Some(remote) = socket.remote_endpoint() {
						let remote_addr = {
							let IpAddress::Ipv6(v6) = remote.addr;
							SocketAddrV6::new(Ipv6Addr::from(v6.0), remote.port, 0, 0)
						};

						let stream = VirtualTcpStream::new(self.device.clone(), self.handle);
						return Ok((stream, remote_addr));
					}
				}
			}

			tokio::time::sleep(std::time::Duration::from_millis(10)).await;
			self.device.poll();
		}
	}

	pub fn local_addr(&self) -> SocketAddrV6 {
		self.local_addr
	}
}

pub struct VirtualTcpStream {
	device: VirtualDevice,
	handle: SocketHandle,
}

impl VirtualTcpStream {
	pub(crate) fn new(device: VirtualDevice, handle: SocketHandle) -> Self {
		Self { device, handle }
	}

	pub async fn wait_connected(&self) -> Result<()> {
		loop {
			let state = self
				.device
				.socket_state(self.handle)
				.ok_or_else(|| EngineError::TcpConnect("socket not found".to_string()))?;

			match state {
				TcpState::Established => return Ok(()),
				TcpState::Closed | TcpState::Closing | TcpState::TimeWait => {
					return Err(EngineError::TcpConnect("connection failed".to_string()))
				}
				_ => {
					tokio::time::sleep(std::time::Duration::from_millis(10)).await;
					self.device.poll();
				}
			}
		}
	}

	fn poll_read_inner(&self, buf: &mut [u8]) -> io::Result<usize> {
		let mut inner = self
			.device
			.inner
			.lock()
			.map_err(|e| io::Error::other(format!("lock poisoned: {}", e)))?;

		self.device.poll_iface(&mut inner);

		let socket = inner.sockets.get_mut::<TcpSocket>(self.handle);

		if socket.can_recv() {
			match socket.recv_slice(buf) {
				Ok(n) => Ok(n),
				Err(e) => Err(io::Error::other(format!("{}", e))),
			}
		} else if socket.state() == TcpState::Established {
			Err(io::Error::new(io::ErrorKind::WouldBlock, "no data"))
		} else {
			Ok(0)
		}
	}

	fn poll_write_inner(&self, buf: &[u8]) -> io::Result<usize> {
		let mut inner = self
			.device
			.inner
			.lock()
			.map_err(|e| io::Error::other(format!("lock poisoned: {}", e)))?;

		let socket = inner.sockets.get_mut::<TcpSocket>(self.handle);

		if socket.can_send() {
			match socket.send_slice(buf) {
				Ok(n) => {
					self.device.poll_iface(&mut inner);
					Ok(n)
				}
				Err(e) => Err(io::Error::other(format!("{}", e))),
			}
		} else if socket.state() == TcpState::Established {
			Err(io::Error::new(io::ErrorKind::WouldBlock, "buffer full"))
		} else {
			Err(io::Error::new(io::ErrorKind::NotConnected, "not connected"))
		}
	}

	fn poll_flush_inner(&self) -> io::Result<()> {
		self.device.poll();
		Ok(())
	}
}

impl AsyncRead for VirtualTcpStream {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut ReadBuf<'_>,
	) -> Poll<io::Result<()>> {
		match self.poll_read_inner(buf.initialize_unfilled()) {
			Ok(n) => {
				buf.advance(n);
				Poll::Ready(Ok(()))
			}
			Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
				self.device.register_waker(cx.waker().clone());
				Poll::Pending
			}
			Err(e) => Poll::Ready(Err(e)),
		}
	}
}

impl AsyncWrite for VirtualTcpStream {
	fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
		match self.poll_write_inner(buf) {
			Ok(n) => Poll::Ready(Ok(n)),
			Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
				self.device.register_waker(cx.waker().clone());
				Poll::Pending
			}
			Err(e) => Poll::Ready(Err(e)),
		}
	}

	fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		Poll::Ready(self.poll_flush_inner())
	}

	fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		let inner = self
			.device
			.inner
			.lock()
			.map_err(|e| io::Error::other(format!("lock poisoned: {}", e)))?;

		let socket = inner.sockets.get::<TcpSocket>(self.handle);
		if socket.state() == TcpState::Established {
			drop(inner);
			let mut inner = self
				.device
				.inner
				.lock()
				.map_err(|e| io::Error::other(format!("lock poisoned: {}", e)))?;
			let socket = inner.sockets.get_mut::<TcpSocket>(self.handle);
			socket.close();
		}

		Poll::Ready(Ok(()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_virtual_device_creation() {
		let addr: Ipv6Addr = "fd7a:115c:a1e0::1".parse().unwrap();
		let device = VirtualDevice::new(addr, 1280).unwrap();

		assert_eq!(device.address(), addr);
		assert_eq!(device.mtu(), 1280);
	}

	#[test]
	fn test_virtual_device_poll() {
		let addr: Ipv6Addr = "fd7a:115c:a1e0::1".parse().unwrap();
		let device = VirtualDevice::new(addr, 1280).unwrap();

		device.poll();
	}

	#[test]
	fn test_virtual_device_clone() {
		let addr: Ipv6Addr = "fd7a:115c:a1e0::1".parse().unwrap();
		let device1 = VirtualDevice::new(addr, 1280).unwrap();
		let device2 = device1.clone();

		assert_eq!(device1.address(), device2.address());
		assert_eq!(device1.mtu(), device2.mtu());
	}
}
