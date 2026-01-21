// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod commands;

pub use commands::{
	compare::CompareArgs, draw::DrawArgs, duplicate::DuplicateArgs, edit::EditArgs, knot::KnotArgs,
	mark::MarkArgs, mend::MendArgs, pin::PinArgs, ply::PlyArgs, rethread::RethreadArgs,
	show::ShowArgs, shuttle::ShuttleArgs, snip::SnipArgs, stitch::StitchArgs, tension::TensionArgs,
	tension_log::TensionLogArgs, trace::TraceArgs, unpick::UnpickArgs, unravel::UnravelArgs,
	untangle::UntangleArgs, wind::WindArgs,
};

#[derive(Debug, clap::Subcommand)]
pub enum SpoolCommands {
	Wind(commands::wind::WindArgs),
	Stitch(commands::stitch::StitchArgs),
	Knot(commands::knot::KnotArgs),
	Mark(commands::mark::MarkArgs),
	Trace(commands::trace::TraceArgs),
	Compare(commands::compare::CompareArgs),
	Tension(commands::tension::TensionArgs),
	Rethread(commands::rethread::RethreadArgs),
	Ply(commands::ply::PlyArgs),
	Unravel(commands::unravel::UnravelArgs),
	Snip(commands::snip::SnipArgs),
	Mend(commands::mend::MendArgs),
	Pin(commands::pin::PinArgs),
	Shuttle(commands::shuttle::ShuttleArgs),
	Draw(commands::draw::DrawArgs),
	TensionLog(commands::tension_log::TensionLogArgs),
	Unpick(commands::unpick::UnpickArgs),
	Untangle(commands::untangle::UntangleArgs),
	Show(commands::show::ShowArgs),
	Edit(commands::edit::EditArgs),
	Duplicate(commands::duplicate::DuplicateArgs),
}

pub async fn run(cmd: &SpoolCommands) -> anyhow::Result<()> {
	match cmd {
		SpoolCommands::Wind(args) => commands::wind::run(args.clone()).await,
		SpoolCommands::Stitch(args) => commands::stitch::run(args.clone()).await,
		SpoolCommands::Knot(args) => commands::knot::run(args.clone()).await,
		SpoolCommands::Mark(args) => commands::mark::run(args.clone()).await,
		SpoolCommands::Trace(args) => commands::trace::run(args.clone()).await,
		SpoolCommands::Compare(args) => commands::compare::run(args.clone()).await,
		SpoolCommands::Tension(args) => commands::tension::run(args.clone()).await,
		SpoolCommands::Rethread(args) => commands::rethread::run(args.clone()).await,
		SpoolCommands::Ply(args) => commands::ply::run(args.clone()).await,
		SpoolCommands::Unravel(args) => commands::unravel::run(args.clone()).await,
		SpoolCommands::Snip(args) => commands::snip::run(args.clone()).await,
		SpoolCommands::Mend(args) => commands::mend::run(args.clone()).await,
		SpoolCommands::Pin(args) => commands::pin::run(args.clone()).await,
		SpoolCommands::Shuttle(args) => commands::shuttle::run(args.clone()).await,
		SpoolCommands::Draw(args) => commands::draw::run(args.clone()).await,
		SpoolCommands::TensionLog(args) => commands::tension_log::run(args.clone()).await,
		SpoolCommands::Unpick(args) => commands::unpick::run(args.clone()).await,
		SpoolCommands::Untangle(args) => commands::untangle::run(args.clone()).await,
		SpoolCommands::Show(args) => commands::show::run(args.clone()).await,
		SpoolCommands::Edit(args) => commands::edit::run(args.clone()).await,
		SpoolCommands::Duplicate(args) => commands::duplicate::run(args.clone()).await,
	}
}
