// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use super::ast::{CompareOp, Filter, LogicalOp};
use crate::error::ScimError;
use winnow::ascii::{alpha1, alphanumeric1, space0, space1};
use winnow::combinator::{alt, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::take_while;

pub struct FilterParser;

impl FilterParser {
	pub fn parse(input: &str) -> Result<Filter, ScimError> {
		parse_filter
			.parse(input.trim())
			.map_err(|e| ScimError::InvalidFilter(format!("{:?}", e)))
	}
}

fn parse_filter(input: &mut &str) -> Result<Filter, ContextError> {
	parse_or_expr(input)
}

fn parse_or_expr(input: &mut &str) -> Result<Filter, ContextError> {
	let left = parse_and_expr(input)?;
	let mut result = left;

	loop {
		let checkpoint = *input;
		let space_result: Result<&str, ContextError> = space1.parse_next(input);
		if space_result.is_err() {
			*input = checkpoint;
			break;
		}
		let or_result: Result<&str, ContextError> = winnow::ascii::Caseless("or").parse_next(input);
		if or_result.is_err() {
			*input = checkpoint;
			break;
		}
		let _: &str = space1.parse_next(input)?;
		let right = parse_and_expr(input)?;
		result = Filter::Logical {
			op: LogicalOp::Or,
			left: Box::new(result),
			right: Box::new(right),
		};
	}

	Ok(result)
}

fn parse_and_expr(input: &mut &str) -> Result<Filter, ContextError> {
	let left = parse_not_expr(input)?;
	let mut result = left;

	loop {
		let checkpoint = *input;
		let space_result: Result<&str, ContextError> = space1.parse_next(input);
		if space_result.is_err() {
			*input = checkpoint;
			break;
		}
		let and_result: Result<&str, ContextError> = winnow::ascii::Caseless("and").parse_next(input);
		if and_result.is_err() {
			*input = checkpoint;
			break;
		}
		let _: &str = space1.parse_next(input)?;
		let right = parse_not_expr(input)?;
		result = Filter::Logical {
			op: LogicalOp::And,
			left: Box::new(result),
			right: Box::new(right),
		};
	}

	Ok(result)
}

fn parse_not_expr(input: &mut &str) -> Result<Filter, ContextError> {
	let checkpoint = *input;
	let not_result: Result<&str, ContextError> = winnow::ascii::Caseless("not").parse_next(input);
	if not_result.is_ok() {
		let space_result: Result<&str, ContextError> = space1.parse_next(input);
		if space_result.is_ok() {
			let expr = parse_atom(input)?;
			return Ok(Filter::Not(Box::new(expr)));
		}
	}
	*input = checkpoint;
	parse_atom(input)
}

fn parse_atom(input: &mut &str) -> Result<Filter, ContextError> {
	let _: &str = space0.parse_next(input)?;

	if input.starts_with('(') {
		let _ = '('.parse_next(input)?;
		let _: &str = space0.parse_next(input)?;
		let filter = parse_filter(input)?;
		let _: &str = space0.parse_next(input)?;
		let _ = ')'.parse_next(input)?;
		return Ok(Filter::Group(Box::new(filter)));
	}

	parse_comparison(input)
}

fn parse_comparison(input: &mut &str) -> Result<Filter, ContextError> {
	let attr_path = parse_attr_path(input)?;
	let _: &str = space1.parse_next(input)?;

	let checkpoint = *input;
	let pr_result: Result<&str, ContextError> = winnow::ascii::Caseless("pr").parse_next(input);
	if pr_result.is_ok() {
		return Ok(Filter::Compare {
			attr_path,
			op: CompareOp::Pr,
			value: None,
		});
	}
	*input = checkpoint;

	let op = parse_compare_op(input)?;
	let _: &str = space1.parse_next(input)?;
	let value = parse_value(input)?;

	Ok(Filter::Compare {
		attr_path,
		op,
		value: Some(value),
	})
}

fn parse_attr_path(input: &mut &str) -> Result<String, ContextError> {
	let first: &str = alpha1.parse_next(input)?;
	let rest: String = repeat(
		0..,
		alt((
			alphanumeric1,
			".".map(|_: &str| "."),
			":".map(|_: &str| ":"),
		)),
	)
	.fold(String::new, |mut acc, s: &str| {
		acc.push_str(s);
		acc
	})
	.parse_next(input)?;
	Ok(format!("{}{}", first, rest))
}

fn parse_compare_op(input: &mut &str) -> Result<CompareOp, ContextError> {
	alt((
		winnow::ascii::Caseless("eq").map(|_| CompareOp::Eq),
		winnow::ascii::Caseless("ne").map(|_| CompareOp::Ne),
		winnow::ascii::Caseless("co").map(|_| CompareOp::Co),
		winnow::ascii::Caseless("sw").map(|_| CompareOp::Sw),
		winnow::ascii::Caseless("ew").map(|_| CompareOp::Ew),
		winnow::ascii::Caseless("gt").map(|_| CompareOp::Gt),
		winnow::ascii::Caseless("ge").map(|_| CompareOp::Ge),
		winnow::ascii::Caseless("lt").map(|_| CompareOp::Lt),
		winnow::ascii::Caseless("le").map(|_| CompareOp::Le),
	))
	.parse_next(input)
}

fn parse_value(input: &mut &str) -> Result<String, ContextError> {
	if input.starts_with('"') {
		let _ = '"'.parse_next(input)?;
		let value: String = take_while(0.., |c| c != '"').parse_next(input)?.to_string();
		let _ = '"'.parse_next(input)?;
		Ok(value)
	} else {
		let value: String = take_while(1.., |c: char| !c.is_whitespace() && c != ')')
			.parse_next(input)?
			.to_string();
		Ok(value)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_simple_eq() {
		let result = FilterParser::parse(r#"userName eq "john""#).unwrap();
		assert!(matches!(
			result,
			Filter::Compare {
				op: CompareOp::Eq,
				..
			}
		));
	}

	#[test]
	fn test_and_expr() {
		let result = FilterParser::parse(r#"userName eq "john" and active eq true"#).unwrap();
		assert!(matches!(
			result,
			Filter::Logical {
				op: LogicalOp::And,
				..
			}
		));
	}

	#[test]
	fn test_pr_operator() {
		let result = FilterParser::parse("emails pr").unwrap();
		assert!(matches!(
			result,
			Filter::Compare {
				op: CompareOp::Pr,
				value: None,
				..
			}
		));
	}
}
