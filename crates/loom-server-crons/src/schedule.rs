// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Schedule parsing and next run calculation for cron monitors.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::str::FromStr;

use loom_crons_core::MonitorSchedule;

use crate::error::{CronsServerError, Result};

/// Convert a standard 5-field Unix cron expression to the 7-field format
/// expected by the `cron` crate.
///
/// 5-field format: minute hour day-of-month month day-of-week
/// 7-field format: second minute hour day-of-month month day-of-week year
///
/// We add "0" for seconds (run at :00 of each minute) and "*" for year (any year).
fn convert_to_cron_crate_format(expression: &str) -> String {
	// Check if it's already a 6 or 7 field expression
	let field_count = expression.split_whitespace().count();
	if field_count >= 6 {
		// Already in extended format, use as-is
		expression.to_string()
	} else if field_count == 5 {
		// Standard 5-field Unix cron - convert to 7-field
		format!("0 {} *", expression)
	} else {
		// Invalid format, return as-is and let the parser error
		expression.to_string()
	}
}

/// Calculate the next expected run time for a monitor.
///
/// For cron schedules, parses the cron expression and finds the next occurrence
/// after the given time in the specified timezone.
///
/// For interval schedules, simply adds the interval duration to the given time.
///
/// # Arguments
///
/// * `schedule` - The monitor's schedule configuration
/// * `timezone` - IANA timezone string (e.g., "America/New_York", "UTC")
/// * `after` - Calculate next run after this time (typically now or last check-in)
///
/// # Returns
///
/// The next expected run time in UTC.
///
/// # Errors
///
/// Returns an error if:
/// - The cron expression is invalid
/// - The timezone string is invalid
/// - No next run can be calculated (shouldn't happen for valid schedules)
pub fn calculate_next_expected(
	schedule: &MonitorSchedule,
	timezone: &str,
	after: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
	match schedule {
		MonitorSchedule::Cron { expression } => {
			// Convert to 7-field format expected by cron crate
			let cron_expr = convert_to_cron_crate_format(expression);

			// Parse the cron expression
			let cron_schedule = Schedule::from_str(&cron_expr)
				.map_err(|e| CronsServerError::InvalidCronExpression(e.to_string()))?;

			// Parse the timezone
			let tz: Tz = timezone
				.parse()
				.map_err(|_| CronsServerError::InvalidTimezone(timezone.to_string()))?;

			// Convert the after time to the monitor's timezone
			let local_after = after.with_timezone(&tz);

			// Find the next occurrence
			let next_local = cron_schedule.after(&local_after).next().ok_or_else(|| {
				CronsServerError::Internal("No next run time found for cron schedule".to_string())
			})?;

			// Convert back to UTC
			Ok(next_local.with_timezone(&Utc))
		}
		MonitorSchedule::Interval { minutes } => {
			// For intervals, simply add the duration
			Ok(after + Duration::minutes(*minutes as i64))
		}
	}
}

/// Validate a cron expression without calculating a next run.
///
/// # Arguments
///
/// * `expression` - The cron expression to validate
///
/// # Returns
///
/// `Ok(())` if valid, `Err` with details if invalid.
pub fn validate_cron_expression(expression: &str) -> Result<()> {
	let cron_expr = convert_to_cron_crate_format(expression);
	Schedule::from_str(&cron_expr)
		.map_err(|e| CronsServerError::InvalidCronExpression(e.to_string()))?;
	Ok(())
}

/// Validate a timezone string.
///
/// # Arguments
///
/// * `timezone` - IANA timezone string to validate
///
/// # Returns
///
/// `Ok(())` if valid, `Err` with details if invalid.
pub fn validate_timezone(timezone: &str) -> Result<()> {
	let _: Tz = timezone
		.parse()
		.map_err(|_| CronsServerError::InvalidTimezone(timezone.to_string()))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::TimeZone;

	#[test]
	fn test_cron_daily_midnight() {
		let schedule = MonitorSchedule::Cron {
			expression: "0 0 * * *".to_string(),
		};

		// 2026-01-19 10:30:00 UTC
		let after = Utc.with_ymd_and_hms(2026, 1, 19, 10, 30, 0).unwrap();

		let next = calculate_next_expected(&schedule, "UTC", after).unwrap();

		// Should be 2026-01-20 00:00:00 UTC
		assert_eq!(next.date_naive().to_string(), "2026-01-20");
		assert_eq!(next.time().to_string(), "00:00:00");
	}

	#[test]
	fn test_cron_every_15_minutes() {
		let schedule = MonitorSchedule::Cron {
			expression: "*/15 * * * *".to_string(),
		};

		// 2026-01-19 10:32:00 UTC
		let after = Utc.with_ymd_and_hms(2026, 1, 19, 10, 32, 0).unwrap();

		let next = calculate_next_expected(&schedule, "UTC", after).unwrap();

		// Should be 2026-01-19 10:45:00 UTC
		assert_eq!(next.date_naive().to_string(), "2026-01-19");
		assert_eq!(next.time().to_string(), "10:45:00");
	}

	#[test]
	fn test_cron_with_timezone() {
		let schedule = MonitorSchedule::Cron {
			expression: "0 9 * * *".to_string(), // 9am daily
		};

		// 2026-01-19 20:00:00 UTC (which is 2026-01-20 07:00:00 Sydney)
		let after = Utc.with_ymd_and_hms(2026, 1, 19, 20, 0, 0).unwrap();

		let next = calculate_next_expected(&schedule, "Australia/Sydney", after).unwrap();

		// 9am Sydney on Jan 20 = 2026-01-19 22:00:00 UTC (AEDT is UTC+11)
		assert_eq!(next.date_naive().to_string(), "2026-01-19");
		assert_eq!(next.time().to_string(), "22:00:00");
	}

	#[test]
	fn test_interval_30_minutes() {
		let schedule = MonitorSchedule::Interval { minutes: 30 };

		// 2026-01-19 10:30:00 UTC
		let after = Utc.with_ymd_and_hms(2026, 1, 19, 10, 30, 0).unwrap();

		let next = calculate_next_expected(&schedule, "UTC", after).unwrap();

		// Should be 2026-01-19 11:00:00 UTC
		assert_eq!(next.date_naive().to_string(), "2026-01-19");
		assert_eq!(next.time().to_string(), "11:00:00");
	}

	#[test]
	fn test_invalid_cron_expression() {
		let schedule = MonitorSchedule::Cron {
			expression: "invalid cron".to_string(),
		};

		let after = Utc::now();
		let result = calculate_next_expected(&schedule, "UTC", after);

		assert!(result.is_err());
	}

	#[test]
	fn test_invalid_timezone() {
		let schedule = MonitorSchedule::Cron {
			expression: "0 0 * * *".to_string(),
		};

		let after = Utc::now();
		let result = calculate_next_expected(&schedule, "Invalid/Timezone", after);

		assert!(result.is_err());
	}

	#[test]
	fn test_validate_cron_expression_valid() {
		assert!(validate_cron_expression("0 0 * * *").is_ok());
		assert!(validate_cron_expression("*/15 * * * *").is_ok());
		assert!(validate_cron_expression("0 9 * * 1-5").is_ok());
	}

	#[test]
	fn test_validate_cron_expression_invalid() {
		assert!(validate_cron_expression("invalid").is_err());
		assert!(validate_cron_expression("60 0 * * *").is_err()); // minute > 59
		assert!(validate_cron_expression("* * * *").is_err()); // missing field
	}

	#[test]
	fn test_validate_timezone_valid() {
		assert!(validate_timezone("UTC").is_ok());
		assert!(validate_timezone("America/New_York").is_ok());
		assert!(validate_timezone("Australia/Sydney").is_ok());
	}

	#[test]
	fn test_validate_timezone_invalid() {
		assert!(validate_timezone("Invalid/Timezone").is_err());
		assert!(validate_timezone("Not_A_Real_TZ").is_err());
	}
}
