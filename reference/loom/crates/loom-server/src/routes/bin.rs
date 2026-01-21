// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Binary directory listing HTTP handler.

use axum::{extract::Request, http::StatusCode, response::IntoResponse};

/// Handler to list files in the /bin directory
/// Only shows the index for requests to `/bin/` (trailing slash).
/// Returns 404 for specific file paths that don't exist.
///
/// Note: When used as a fallback for ServeDir nested at `/bin`, the request
/// path is relative to the nest point. So `/bin` becomes `/` and `/bin/foo`
/// becomes `/foo`.
pub async fn list_bin_directory(request: Request) -> impl IntoResponse {
	let request_path = request.uri().path();

	// When nested under /bin via nest_service, paths are relative:
	// - /bin or /bin/ becomes / or empty
	// - /bin/does-not-exist becomes /does-not-exist
	// Only show directory listing for the root path (the /bin directory itself)
	if request_path != "/" && !request_path.is_empty() {
		return (
			StatusCode::NOT_FOUND,
			[("Content-Type", "text/plain; charset=utf-8")],
			"404 Not Found".to_string(),
		);
	}

	let bin_dir = std::env::var("LOOM_SERVER_BIN_DIR").unwrap_or_else(|_| "./bin".to_string());
	let path = std::path::Path::new(&bin_dir);

	let mut entries = Vec::new();

	if path.exists() && path.is_dir() {
		if let Ok(read_dir) = std::fs::read_dir(path) {
			for entry in read_dir.flatten() {
				let name = entry.file_name().to_string_lossy().to_string();
				let metadata = entry.metadata().ok();
				let is_dir = metadata.as_ref().is_some_and(|m| m.is_dir());
				let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

				entries.push((name, is_dir, size));
			}
		}
	}

	entries.sort_by(|a, b| match (a.1, b.1) {
		(true, false) => std::cmp::Ordering::Less,
		(false, true) => std::cmp::Ordering::Greater,
		_ => a.0.cmp(&b.0),
	});

	let mut html = String::from(
		r#"<!DOCTYPE html>
<html>
<head>
<title>Index of /bin/</title>
<style>
body { font-family: monospace; margin: 2em; }
table { border-collapse: collapse; }
th, td { text-align: left; padding: 0.25em 1em; }
th { border-bottom: 1px solid #ccc; }
a { text-decoration: none; }
a:hover { text-decoration: underline; }
.dir { font-weight: bold; }
.size { text-align: right; }
</style>
</head>
<body>
<h1>Index of /bin/</h1>
<table>
<tr><th>Name</th><th>Size</th></tr>
"#,
	);

	for (name, is_dir, size) in entries {
		let display_name = if is_dir {
			format!("{name}/")
		} else {
			name.clone()
		};
		let size_str = if is_dir {
			"-".to_string()
		} else {
			format_size(size)
		};
		let class = if is_dir { " class=\"dir\"" } else { "" };
		html.push_str(&format!(
			r#"<tr><td{class}><a href="/bin/{name}">{display_name}</a></td><td class="size">{size_str}</td></tr>
"#
		));
	}

	html.push_str("</table>\n</body>\n</html>");

	(
		StatusCode::OK,
		[("Content-Type", "text/html; charset=utf-8")],
		html,
	)
}

fn format_size(size: u64) -> String {
	const KB: u64 = 1024;
	const MB: u64 = KB * 1024;
	const GB: u64 = MB * 1024;

	if size >= GB {
		format!("{:.1}G", size as f64 / GB as f64)
	} else if size >= MB {
		format!("{:.1}M", size as f64 / MB as f64)
	} else if size >= KB {
		format!("{:.1}K", size as f64 / KB as f64)
	} else {
		format!("{size}")
	}
}
