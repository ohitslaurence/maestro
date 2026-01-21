// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareOp {
	Eq,
	Ne,
	Co,
	Sw,
	Ew,
	Gt,
	Ge,
	Lt,
	Le,
	Pr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalOp {
	And,
	Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
	Compare {
		attr_path: String,
		op: CompareOp,
		value: Option<String>,
	},
	Logical {
		op: LogicalOp,
		left: Box<Filter>,
		right: Box<Filter>,
	},
	Not(Box<Filter>),
	Group(Box<Filter>),
}
