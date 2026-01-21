// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::io::{self, Write};
use tracing_subscriber::fmt::MakeWriter;

/// A writer that redacts secrets before writing to the underlying writer.
pub struct RedactingWriter<W: Write> {
	inner: W,
	buffer: Vec<u8>,
}

impl<W: Write> Drop for RedactingWriter<W> {
	fn drop(&mut self) {
		let _ = self.flush();
	}
}

impl<W: Write> Write for RedactingWriter<W> {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		self.buffer.extend_from_slice(buf);

		while let Some(newline_pos) = self.buffer.iter().position(|&b| b == b'\n') {
			let line = &self.buffer[..=newline_pos];
			if let Ok(s) = std::str::from_utf8(line) {
				let redacted = loom_redact::redact(s);
				self.inner.write_all(redacted.as_bytes())?;
			} else {
				// Non-UTF8: use lossy decoding to still redact ASCII secrets
				let s = String::from_utf8_lossy(line);
				let redacted = loom_redact::redact(&s);
				self.inner.write_all(redacted.as_bytes())?;
			}
			self.buffer.drain(..=newline_pos);
		}

		Ok(buf.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		if !self.buffer.is_empty() {
			if let Ok(s) = std::str::from_utf8(&self.buffer) {
				let redacted = loom_redact::redact(s);
				self.inner.write_all(redacted.as_bytes())?;
			} else {
				let s = String::from_utf8_lossy(&self.buffer);
				let redacted = loom_redact::redact(&s);
				self.inner.write_all(redacted.as_bytes())?;
			}
			self.buffer.clear();
		}
		self.inner.flush()
	}
}

/// A MakeWriter that wraps another MakeWriter and redacts secrets.
pub struct RedactingMakeWriter<M> {
	inner: M,
}

impl<M> RedactingMakeWriter<M> {
	pub fn new(inner: M) -> Self {
		Self { inner }
	}
}

impl<'a, M> MakeWriter<'a> for RedactingMakeWriter<M>
where
	M: MakeWriter<'a>,
{
	type Writer = RedactingWriter<M::Writer>;

	fn make_writer(&'a self) -> Self::Writer {
		RedactingWriter {
			inner: self.inner.make_writer(),
			buffer: Vec::new(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Cursor;

	fn github_pat() -> String {
		format!("ghp_{}", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8")
	}

	fn aws_key() -> String {
		format!("AKIA{}", "Z7VRSQ5TJN2XMPLQ")
	}

	#[test]
	fn test_redacting_writer_single_line() {
		let mut output = Vec::new();
		let secret = github_pat();
		{
			let mut writer = RedactingWriter {
				inner: Cursor::new(&mut output),
				buffer: Vec::new(),
			};
			write!(writer, "token={}\n", secret).unwrap();
		}
		let result = String::from_utf8(output).unwrap();
		assert!(result.contains("[REDACTED:"), "Result: {}", result);
		assert!(!result.contains(&secret));
	}

	#[test]
	fn test_redacting_writer_multiple_lines() {
		let mut output = Vec::new();
		let secret = github_pat();
		{
			let mut writer = RedactingWriter {
				inner: Cursor::new(&mut output),
				buffer: Vec::new(),
			};
			writer.write_all(b"line1\n").unwrap();
			write!(writer, "GITHUB_TOKEN={}\n", secret).unwrap();
		}
		let result = String::from_utf8(output).unwrap();
		assert!(result.contains("line1\n"));
		assert!(result.contains("[REDACTED:"), "Result: {}", result);
		assert!(!result.contains(&secret));
	}

	#[test]
	fn test_redacting_writer_partial_line() {
		let mut output = Vec::new();
		let secret = aws_key();
		{
			let mut writer = RedactingWriter {
				inner: Cursor::new(&mut output),
				buffer: Vec::new(),
			};
			writer.write_all(b"AWS_ACCESS_KEY_ID=").unwrap();
			write!(writer, "{}\n", secret).unwrap();
		}
		let result = String::from_utf8(output).unwrap();
		assert!(result.contains("[REDACTED:"), "Result: {}", result);
		assert!(!result.contains(&secret));
	}

	#[test]
	fn test_redacting_writer_flush_incomplete() {
		let mut output = Vec::new();
		let secret = github_pat();
		{
			let mut writer = RedactingWriter {
				inner: Cursor::new(&mut output),
				buffer: Vec::new(),
			};
			write!(writer, "GITHUB_TOKEN={}", secret).unwrap();
		}
		let result = String::from_utf8(output).unwrap();
		assert!(result.contains("[REDACTED:"), "Result: {}", result);
		assert!(!result.contains(&secret));
	}

	#[test]
	fn test_redacting_writer_no_secrets() {
		let mut output = Vec::new();
		{
			let mut writer = RedactingWriter {
				inner: Cursor::new(&mut output),
				buffer: Vec::new(),
			};
			writer.write_all(b"hello world\n").unwrap();
		}
		let result = String::from_utf8(output).unwrap();
		assert_eq!(result, "hello world\n");
	}

	struct MockMakeWriter {
		output: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
	}

	impl<'a> MakeWriter<'a> for MockMakeWriter {
		type Writer = MockWriter;

		fn make_writer(&'a self) -> Self::Writer {
			MockWriter {
				output: self.output.clone(),
			}
		}
	}

	struct MockWriter {
		output: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
	}

	impl Write for MockWriter {
		fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
			self.output.lock().unwrap().extend_from_slice(buf);
			Ok(buf.len())
		}

		fn flush(&mut self) -> io::Result<()> {
			Ok(())
		}
	}

	#[test]
	fn test_redacting_make_writer() {
		let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
		let make_writer = RedactingMakeWriter::new(MockMakeWriter {
			output: output.clone(),
		});
		let secret = github_pat();

		{
			let mut writer = make_writer.make_writer();
			write!(writer, "GITHUB_TOKEN={}\n", secret).unwrap();
		}

		let result = String::from_utf8(output.lock().unwrap().clone()).unwrap();
		assert!(result.contains("[REDACTED:"), "Result: {}", result);
		assert!(!result.contains(&secret));
	}
}
