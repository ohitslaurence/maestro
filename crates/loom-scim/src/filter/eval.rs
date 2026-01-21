// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use super::ast::{CompareOp, Filter, LogicalOp};

pub fn evaluate_filter<F>(filter: &Filter, get_attr: &F) -> bool
where
	F: Fn(&str) -> Option<String>,
{
	match filter {
		Filter::Compare {
			attr_path,
			op,
			value,
		} => {
			let attr_value = get_attr(attr_path);
			match op {
				CompareOp::Pr => attr_value.is_some(),
				CompareOp::Eq => attr_value
					.as_deref()
					.map(|v| v.eq_ignore_ascii_case(value.as_deref().unwrap_or("")))
					.unwrap_or(false),
				CompareOp::Ne => attr_value
					.as_deref()
					.map(|v| !v.eq_ignore_ascii_case(value.as_deref().unwrap_or("")))
					.unwrap_or(true),
				CompareOp::Co => {
					let val = value.as_deref().unwrap_or("").to_lowercase();
					attr_value
						.as_deref()
						.map(|v| v.to_lowercase().contains(&val))
						.unwrap_or(false)
				}
				CompareOp::Sw => {
					let val = value.as_deref().unwrap_or("").to_lowercase();
					attr_value
						.as_deref()
						.map(|v| v.to_lowercase().starts_with(&val))
						.unwrap_or(false)
				}
				CompareOp::Ew => {
					let val = value.as_deref().unwrap_or("").to_lowercase();
					attr_value
						.as_deref()
						.map(|v| v.to_lowercase().ends_with(&val))
						.unwrap_or(false)
				}
				CompareOp::Gt | CompareOp::Ge | CompareOp::Lt | CompareOp::Le => false,
			}
		}
		Filter::Logical { op, left, right } => {
			let left_result = evaluate_filter(left, get_attr);
			match op {
				LogicalOp::And => left_result && evaluate_filter(right, get_attr),
				LogicalOp::Or => left_result || evaluate_filter(right, get_attr),
			}
		}
		Filter::Not(inner) => !evaluate_filter(inner, get_attr),
		Filter::Group(inner) => evaluate_filter(inner, get_attr),
	}
}
