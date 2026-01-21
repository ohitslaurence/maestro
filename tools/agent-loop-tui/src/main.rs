use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(name = "agent-loop-tui", about = "TUI wrapper for scripts/agent-loop.sh")]
struct Cli {
    #[arg(long, default_value = "logs/agent-loop")]
    log_dir: String,
    #[arg(long, default_value = "scripts/agent-loop.sh")]
    script: String,
    #[arg(trailing_var_arg = true)]
    script_args: Vec<String>,
}

#[derive(Debug, Default)]
struct ReportData {
    spec_path: Option<PathBuf>,
    plan_path: Option<PathBuf>,
    iterations: Option<String>,
    model: Option<String>,
    run_start_ms: Option<i64>,
    last_iter: Option<String>,
    last_iter_tail: Option<PathBuf>,
    last_iter_log: Option<PathBuf>,
    completion_iter: Option<String>,
    completion_mode: Option<String>,
    exit_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SummaryRaw {
    run_id: String,
    start_ms: i64,
    end_ms: i64,
    total_duration_ms: i64,
    iterations_run: i64,
    completed_iteration: Option<i64>,
    avg_duration_ms: i64,
    last_exit_code: i64,
    completion_mode: Option<String>,
    model: String,
    exit_reason: String,
    run_log: String,
    run_report: String,
    prompt_snapshot: String,
    last_iteration_tail: Option<String>,
    last_iteration_log: Option<String>,
}

#[derive(Debug)]
struct SummaryView {
    run_id: String,
    start_ms: i64,
    end_ms: i64,
    total_duration_ms: i64,
    iterations_run: i64,
    completed_iteration: Option<i64>,
    avg_duration_ms: i64,
    last_exit_code: i64,
    completion_mode: Option<String>,
    model: String,
    exit_reason: String,
    run_log: PathBuf,
    run_report: PathBuf,
    prompt_snapshot: PathBuf,
    last_iteration_tail: Option<PathBuf>,
    last_iteration_log: Option<PathBuf>,
}

struct AppState {
    repo_root: PathBuf,
    log_dir: PathBuf,
    known_runs: HashSet<String>,
    run_id: Option<String>,
    run_dir: Option<PathBuf>,
    report_path: Option<PathBuf>,
    summary_path: Option<PathBuf>,
    run_log_path: Option<PathBuf>,
    report_data: ReportData,
    summary: Option<SummaryView>,
    tail_lines: Vec<String>,
    exit_status: Option<ExitStatus>,
    shutdown_requested: bool,
    last_refresh: Instant,
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide).context("enter alternate screen")?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (repo_root, script_path) = resolve_paths(&cli)?;
    let (log_dir, script_args) = prepare_script_args(&cli, &repo_root)?;

    if !script_path.exists() {
        return Err(anyhow!("script not found: {}", script_path.display()));
    }

    fs::create_dir_all(&log_dir).context("create log dir")?;

    let known_runs = list_run_dirs(&log_dir).unwrap_or_default();
    let mut child = spawn_agent_loop(&repo_root, &script_path, &script_args)?;

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("terminal init")?;

    let mut app = AppState {
        repo_root,
        log_dir,
        known_runs,
        run_id: None,
        run_dir: None,
        report_path: None,
        summary_path: None,
        run_log_path: None,
        report_data: ReportData::default(),
        summary: None,
        tail_lines: Vec::new(),
        exit_status: None,
        shutdown_requested: false,
        last_refresh: Instant::now(),
    };

    run_loop(&mut terminal, &mut app, &mut child)?;
    Ok(())
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut AppState, child: &mut Child) -> Result<()> {
    let tick_rate = Duration::from_millis(300);

    loop {
        let timeout = tick_rate
            .checked_sub(app.last_refresh.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout).context("event poll")? {
            if let Event::Key(key) = event::read().context("event read")? {
                if key.kind == KeyEventKind::Press {
                    if handle_key_event(key.code, app, child)? {
                        break;
                    }
                }
            }
        }

        if app.last_refresh.elapsed() >= tick_rate {
            refresh_state(app)?;
            if app.exit_status.is_none() {
                app.exit_status = child.try_wait().context("check child")?;
            }
            terminal.draw(|frame| draw_ui(frame, app)).context("draw ui")?;
            app.last_refresh = Instant::now();
        }

        if app.exit_status.is_some() && app.shutdown_requested {
            break;
        }
    }

    Ok(())
}

fn handle_key_event(code: KeyCode, app: &mut AppState, child: &mut Child) -> Result<bool> {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            if app.exit_status.is_none() {
                send_sigint(child)?;
                app.shutdown_requested = true;
                return Ok(false);
            }
            return Ok(true);
        }
        _ => {}
    }
    Ok(false)
}

fn resolve_paths(cli: &Cli) -> Result<(PathBuf, PathBuf)> {
    let script_path = Path::new(&cli.script);
    let cwd = env::current_dir().context("current dir")?;

    if script_path.is_absolute() {
        let repo_root = infer_repo_root_from_script(script_path).unwrap_or_else(|| cwd.clone());
        return Ok((repo_root, script_path.to_path_buf()));
    }

    let repo_root = find_repo_root(&cwd, script_path)
        .unwrap_or_else(|| cwd.clone());
    Ok((repo_root.clone(), repo_root.join(script_path)))
}

fn prepare_script_args(cli: &Cli, repo_root: &Path) -> Result<(PathBuf, Vec<String>)> {
    let mut args = cli.script_args.clone();
    let mut log_dir = PathBuf::from(&cli.log_dir);

    if let Some(arg_log_dir) = extract_log_dir(&args) {
        log_dir = PathBuf::from(arg_log_dir);
    } else {
        args.push("--log-dir".to_string());
        args.push(cli.log_dir.clone());
    }

    if !args.iter().any(|arg| arg == "--no-gum") {
        args.push("--no-gum".to_string());
    }

    if !args.iter().any(|arg| arg == "--summary-json") {
        args.push("--summary-json".to_string());
    }

    if !args.iter().any(|arg| arg == "--no-wait") {
        args.push("--no-wait".to_string());
    }

    if !log_dir.is_absolute() {
        log_dir = repo_root.join(log_dir);
    }

    Ok((log_dir, args))
}

fn extract_log_dir(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--log-dir" {
            return iter.next().cloned();
        }
    }
    None
}

fn spawn_agent_loop(repo_root: &Path, script_path: &Path, args: &[String]) -> Result<Child> {
    let mut cmd = Command::new(script_path);
    cmd.current_dir(repo_root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    cmd.spawn().context("spawn agent loop")
}

fn refresh_state(app: &mut AppState) -> Result<()> {
    if app.run_id.is_none() {
        if let Some(run_dir_name) = detect_new_run_dir(&app.log_dir, &mut app.known_runs)? {
            let run_dir = app.log_dir.join(&run_dir_name);
            let run_id = run_dir_name.trim_start_matches("run-").to_string();
            app.run_id = Some(run_id);
            app.run_dir = Some(run_dir.clone());
            app.report_path = Some(run_dir.join("report.tsv"));
            app.summary_path = Some(run_dir.join("summary.json"));
            app.run_log_path = Some(run_dir.join("run.log"));
        }
    }

    if let Some(report_path) = app.report_path.as_ref() {
        if report_path.exists() {
            app.report_data = parse_report(report_path, &app.repo_root)?;
        }
    }

    if let Some(summary_path) = app.summary_path.as_ref() {
        app.summary = read_summary(summary_path, &app.repo_root)?;
    }

    app.tail_lines = load_tail_lines(app)?;

    Ok(())
}

fn list_run_dirs(log_dir: &Path) -> Result<HashSet<String>> {
    let mut runs = HashSet::new();
    for entry in fs::read_dir(log_dir).context("read log dir")? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("run-") {
            runs.insert(name.to_string());
        }
    }
    Ok(runs)
}

fn detect_new_run_dir(log_dir: &Path, known: &mut HashSet<String>) -> Result<Option<String>> {
    let mut candidates: Vec<(String, SystemTime)> = Vec::new();
    for entry in fs::read_dir(log_dir).context("read log dir")? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if !name_str.starts_with("run-") || known.contains(&name_str) {
            continue;
        }
        let modified = entry.metadata().and_then(|meta| meta.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((name_str, modified));
    }

    candidates.sort_by_key(|(_, modified)| *modified);
    if let Some((name, _)) = candidates.pop() {
        known.insert(name.clone());
        return Ok(Some(name));
    }

    Ok(None)
}

fn parse_report(report_path: &Path, repo_root: &Path) -> Result<ReportData> {
    let contents = fs::read_to_string(report_path).context("read report")?;
    let mut data = ReportData::default();

    for (index, line) in contents.lines().enumerate() {
        if index == 0 {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 9 {
            continue;
        }
        let timestamp = parts[0];
        let kind = parts[1];
        let iteration = parts[2];
        let output_path = parts[7];
        let message = parts[8];

        match kind {
            "RUN_START" => {
                if let Ok(parsed) = timestamp.parse::<i64>() {
                    data.run_start_ms = Some(parsed);
                }
                for token in message.split_whitespace() {
                    if let Some(value) = token.strip_prefix("spec=") {
                        data.spec_path = Some(resolve_path(repo_root, value));
                    } else if let Some(value) = token.strip_prefix("plan=") {
                        data.plan_path = Some(resolve_path(repo_root, value));
                    } else if let Some(value) = token.strip_prefix("iterations=") {
                        data.iterations = Some(value.to_string());
                    } else if let Some(value) = token.strip_prefix("model=") {
                        data.model = Some(value.to_string());
                    }
                }
            }
            "ITERATION_END" => {
                if !iteration.is_empty() {
                    data.last_iter = Some(iteration.to_string());
                }
                if !output_path.is_empty() {
                    data.last_iter_log = Some(resolve_path(repo_root, output_path));
                }
            }
            "ITERATION_TAIL" => {
                if !output_path.is_empty() {
                    data.last_iter_tail = Some(resolve_path(repo_root, output_path));
                }
            }
            "COMPLETE_DETECTED" => {
                if !iteration.is_empty() {
                    data.completion_iter = Some(iteration.to_string());
                }
                if let Some(mode) = message.strip_prefix("mode=") {
                    data.completion_mode = Some(mode.to_string());
                }
            }
            "RUN_END" => {
                if let Some(reason) = message.strip_prefix("reason=") {
                    data.exit_reason = Some(reason.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(data)
}

fn read_summary(summary_path: &Path, repo_root: &Path) -> Result<Option<SummaryView>> {
    if !summary_path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(summary_path).context("read summary")?;
    let raw: SummaryRaw = serde_json::from_str(&contents).context("parse summary json")?;
    let summary = SummaryView {
        run_id: raw.run_id,
        start_ms: raw.start_ms,
        end_ms: raw.end_ms,
        total_duration_ms: raw.total_duration_ms,
        iterations_run: raw.iterations_run,
        completed_iteration: raw.completed_iteration,
        avg_duration_ms: raw.avg_duration_ms,
        last_exit_code: raw.last_exit_code,
        completion_mode: raw.completion_mode,
        model: raw.model,
        exit_reason: raw.exit_reason,
        run_log: resolve_path(repo_root, &raw.run_log),
        run_report: resolve_path(repo_root, &raw.run_report),
        prompt_snapshot: resolve_path(repo_root, &raw.prompt_snapshot),
        last_iteration_tail: raw
            .last_iteration_tail
            .map(|path| resolve_path(repo_root, &path)),
        last_iteration_log: raw
            .last_iteration_log
            .map(|path| resolve_path(repo_root, &path)),
    };
    Ok(Some(summary))
}

fn load_tail_lines(app: &AppState) -> Result<Vec<String>> {
    let mut candidate = None;
    if let Some(summary) = app.summary.as_ref() {
        if let Some(path) = summary.last_iteration_tail.as_ref() {
            candidate = Some(path.clone());
        }
    }

    if candidate.is_none() {
        if let Some(path) = app.report_data.last_iter_tail.as_ref() {
            candidate = Some(path.clone());
        }
    }

    if candidate.is_none() {
        if let Some(run_dir) = app.run_dir.as_ref() {
            if let Some(path) = find_latest_tail(run_dir)? {
                candidate = Some(path);
            }
        }
    }

    if let Some(path) = candidate {
        let contents = fs::read_to_string(path).unwrap_or_default();
        return Ok(contents.lines().map(|line| line.to_string()).collect());
    }

    Ok(vec!["Waiting for output...".to_string()])
}

fn find_latest_tail(run_dir: &Path) -> Result<Option<PathBuf>> {
    let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in fs::read_dir(run_dir).context("read run dir")? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".tail.txt") {
            continue;
        }
        let modified = entry.metadata().and_then(|meta| meta.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((entry.path(), modified));
    }
    candidates.sort_by_key(|(_, modified)| *modified);
    Ok(candidates.pop().map(|(path, _)| path))
}

fn resolve_path(repo_root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn find_repo_root(start: &Path, script_rel: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(script_rel).exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn infer_repo_root_from_script(script_path: &Path) -> Option<PathBuf> {
    let scripts_dir = script_path.parent()?;
    if scripts_dir.file_name()? != "scripts" {
        return None;
    }
    scripts_dir.parent().map(|path| path.to_path_buf())
}

fn send_sigint(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    {
        let pid = child.id();
        unsafe {
            libc::kill(pid as i32, libc::SIGINT);
        }
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        child.kill().context("kill child")?;
        return Ok(());
    }
}

fn draw_ui(frame: &mut ratatui::Frame, app: &AppState) {
    let size = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)])
        .split(size);

    draw_header(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);
}

fn draw_header(frame: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let status = status_label(app);
    let header_line = Line::from(vec![
        Span::styled("Agent Loop TUI", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(status, status_style(app)),
    ]);
    let run_line = Line::from(vec![
        Span::styled("Run", Style::default().fg(Color::Gray)),
        Span::raw(": "),
        Span::raw(app.run_id.as_deref().unwrap_or("pending")),
    ]);
    let text = Text::from(vec![header_line, run_line]);
    let block = Block::default().borders(Borders::ALL).title("Status");
    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_body(frame: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let info_lines = build_info_lines(app);
    let info_text = Text::from(info_lines);
    let info_block = Block::default().borders(Borders::ALL).title("Run Info");
    let info_paragraph = Paragraph::new(info_text).block(info_block).wrap(Wrap { trim: false });
    frame.render_widget(info_paragraph, columns[0]);

    let tail_text = build_tail_text(app, columns[1]);
    let tail_block = Block::default().borders(Borders::ALL).title("Last Output");
    let tail_paragraph = Paragraph::new(tail_text).block(tail_block).wrap(Wrap { trim: false });
    frame.render_widget(tail_paragraph, columns[1]);
}

fn draw_footer(frame: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let mut line = String::from("q: quit (sends SIGINT)");
    if app.exit_status.is_some() {
        line = "q: quit".to_string();
    }
    let footer = Paragraph::new(line).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, area);
}

fn build_info_lines(app: &AppState) -> Vec<Line> {
    let mut lines = Vec::new();
    let label_style = Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD);

    let status_line = info_line("Status", &status_label(app), label_style);
    lines.push(status_line);

    lines.push(info_line(
        "Run ID",
        app.run_id.as_deref().unwrap_or("pending"),
        label_style,
    ));

    if let Some(summary) = app.summary.as_ref() {
        lines.push(info_line("Exit", &summary.exit_reason, label_style));
        lines.push(info_line("Model", &summary.model, label_style));
        lines.push(info_line(
            "Iterations",
            &summary.iterations_run.to_string(),
            label_style,
        ));
        if let Some(iter) = summary.completed_iteration {
            lines.push(info_line("Completed", &iter.to_string(), label_style));
        }
        if let Some(mode) = summary.completion_mode.as_ref() {
            lines.push(info_line("Mode", mode, label_style));
        }
        lines.push(info_line(
            "Exit Code",
            &summary.last_exit_code.to_string(),
            label_style,
        ));
        lines.push(info_line(
            "Duration",
            &format_duration_ms(summary.total_duration_ms),
            label_style,
        ));
        lines.push(info_line(
            "Run Log",
            &summary.run_log.display().to_string(),
            label_style,
        ));
    } else {
        if let Some(reason) = app.report_data.exit_reason.as_ref() {
            lines.push(info_line("Exit", reason, label_style));
        }
        if let Some(model) = app.report_data.model.as_ref() {
            lines.push(info_line("Model", model, label_style));
        }
        if let Some(iter) = app.report_data.iterations.as_ref() {
            lines.push(info_line("Max Iter", iter, label_style));
        }
        if let Some(iter) = app.report_data.last_iter.as_ref() {
            lines.push(info_line("Last Iter", iter, label_style));
        }
        if let Some(mode) = app.report_data.completion_mode.as_ref() {
            lines.push(info_line("Mode", mode, label_style));
        }
        if let Some(start_ms) = app.report_data.run_start_ms {
            if app.exit_status.is_none() {
                let elapsed = elapsed_since(start_ms);
                lines.push(info_line("Elapsed", &elapsed, label_style));
            }
        }
        if let Some(run_log) = app.run_log_path.as_ref() {
            lines.push(info_line("Run Log", &run_log.display().to_string(), label_style));
        }
    }

    if let Some(spec) = app.report_data.spec_path.as_ref() {
        lines.push(info_line("Spec", &spec.display().to_string(), label_style));
    }
    if let Some(plan) = app.report_data.plan_path.as_ref() {
        lines.push(info_line("Plan", &plan.display().to_string(), label_style));
    }
    if let Some(run_dir) = app.run_dir.as_ref() {
        lines.push(info_line("Run Dir", &run_dir.display().to_string(), label_style));
    }

    if lines.is_empty() {
        lines.push(Line::from("Waiting for run metadata..."));
    }

    lines
}

fn info_line(label: &str, value: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), style),
        Span::raw(value.to_string()),
    ])
}

fn build_tail_text(app: &AppState, area: Rect) -> Text {
    let available = area.height.saturating_sub(2) as usize;
    let lines = if app.tail_lines.len() > available && available > 0 {
        app.tail_lines[app.tail_lines.len() - available..].to_vec()
    } else {
        app.tail_lines.clone()
    };

    if lines.is_empty() {
        return Text::from(Line::from("Waiting for output..."));
    }

    Text::from(lines.into_iter().map(Line::from).collect::<Vec<Line>>())
}

fn status_label(app: &AppState) -> String {
    if let Some(status) = app.exit_status.as_ref() {
        if status.success() {
            return "Exited (0)".to_string();
        }
        if let Some(code) = status.code() {
            return format!("Exited ({code})");
        }
        return "Exited".to_string();
    }
    if app.shutdown_requested {
        return "Stopping".to_string();
    }
    "Running".to_string()
}

fn status_style(app: &AppState) -> Style {
    if app.exit_status.is_some() {
        return Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
    }
    if app.shutdown_requested {
        return Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    }
    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
}

fn elapsed_since(start_ms: i64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let elapsed = now_ms.saturating_sub(start_ms);
    format_duration_ms(elapsed)
}

fn format_duration_ms(ms: i64) -> String {
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let remaining = seconds % 60;
    if minutes > 0 {
        format!("{minutes}m {remaining}s")
    } else {
        format!("{seconds}s")
    }
}
