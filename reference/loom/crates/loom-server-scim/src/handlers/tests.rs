// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#[cfg(test)]
mod resource_types_tests {
	use crate::handlers::resource_types::{get_resource_type, list_resource_types};
	use axum::extract::Path;

	#[tokio::test]
	async fn test_list_resource_types_returns_two_types() {
		let response = list_resource_types().await;
		let list = response.0;

		assert_eq!(list.total_results, 2);
		assert_eq!(list.resources.len(), 2);

		let ids: Vec<&str> = list.resources.iter().map(|r| r.id.as_str()).collect();
		assert!(ids.contains(&"User"));
		assert!(ids.contains(&"Group"));
	}

	#[tokio::test]
	async fn test_get_resource_type_user() {
		let result = get_resource_type(Path("User".to_string())).await;
		assert!(result.is_ok());
		let resource_type = result.unwrap().0;
		assert_eq!(resource_type.id, "User");
		assert_eq!(resource_type.name, "User");
		assert!(resource_type.endpoint.contains("Users"));
	}

	#[tokio::test]
	async fn test_get_resource_type_group() {
		let result = get_resource_type(Path("Group".to_string())).await;
		assert!(result.is_ok());
		let resource_type = result.unwrap().0;
		assert_eq!(resource_type.id, "Group");
		assert_eq!(resource_type.name, "Group");
		assert!(resource_type.endpoint.contains("Groups"));
	}

	#[tokio::test]
	async fn test_get_resource_type_not_found() {
		let result = get_resource_type(Path("Unknown".to_string())).await;
		assert!(result.is_err());
		assert_eq!(result.unwrap_err(), axum::http::StatusCode::NOT_FOUND);
	}
}

#[cfg(test)]
mod filter_evaluation_tests {
	use loom_scim::{evaluate_filter, FilterParser, ScimUser};

	fn test_user(username: &str, display_name: Option<&str>, active: bool) -> ScimUser {
		ScimUser {
			schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:User".to_string()],
			id: Some(uuid::Uuid::new_v4().to_string()),
			external_id: None,
			user_name: username.to_string(),
			name: None,
			display_name: display_name.map(|s| s.to_string()),
			nick_name: None,
			profile_url: None,
			title: None,
			user_type: None,
			preferred_language: None,
			locale: None,
			timezone: None,
			active,
			emails: vec![],
			phone_numbers: vec![],
			meta: None,
		}
	}

	fn get_user_attr(user: &ScimUser, attr: &str) -> Option<String> {
		match attr.to_lowercase().as_str() {
			"username" => Some(user.user_name.clone()),
			"displayname" => user.display_name.clone(),
			"active" => Some(user.active.to_string()),
			"id" => user.id.clone(),
			_ => None,
		}
	}

	#[test]
	fn test_filter_eq_username() {
		let filter = FilterParser::parse("userName eq \"john\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("jane", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_eq_case_insensitive() {
		let filter = FilterParser::parse("userName eq \"JOHN\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));
	}

	#[test]
	fn test_filter_ne() {
		let filter = FilterParser::parse("userName ne \"john\"").unwrap();
		let user = test_user("jane", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("john", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_co_contains() {
		let filter = FilterParser::parse("userName co \"oh\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("jane", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_sw_starts_with() {
		let filter = FilterParser::parse("userName sw \"jo\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("jane", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_ew_ends_with() {
		let filter = FilterParser::parse("userName ew \"hn\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("jane", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_pr_present() {
		let filter = FilterParser::parse("displayName pr").unwrap();
		let user = test_user("john", Some("John Doe"), true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("jane", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_and() {
		let filter = FilterParser::parse("userName eq \"john\" and active eq \"true\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("john", None, false);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_or() {
		let filter = FilterParser::parse("userName eq \"john\" or userName eq \"jane\"").unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("jane", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));

		let user3 = test_user("bob", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user3, attr
		)));
	}

	#[test]
	fn test_filter_not() {
		let filter = FilterParser::parse("not userName eq \"john\"").unwrap();
		let user = test_user("jane", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("john", None, true);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}

	#[test]
	fn test_filter_grouped() {
		let filter =
			FilterParser::parse("(userName eq \"john\" or userName eq \"jane\") and active eq \"true\"")
				.unwrap();
		let user = test_user("john", None, true);
		assert!(evaluate_filter(&filter, &|attr| get_user_attr(&user, attr)));

		let user2 = test_user("john", None, false);
		assert!(!evaluate_filter(&filter, &|attr| get_user_attr(
			&user2, attr
		)));
	}
}

#[cfg(test)]
mod bulk_tests {
	use crate::handlers::bulk::{parse_path, resolve_bulk_id_refs, ResourcePath};
	use std::collections::HashMap;

	#[test]
	fn test_parse_path_users() {
		let result = parse_path("/Users");
		assert!(matches!(result, Some(ResourcePath::Users)));

		let result = parse_path("Users");
		assert!(matches!(result, Some(ResourcePath::Users)));
	}

	#[test]
	fn test_parse_path_users_id() {
		let result = parse_path("/Users/123");
		assert!(matches!(result, Some(ResourcePath::UsersId(ref id)) if id == "123"));
	}

	#[test]
	fn test_parse_path_groups() {
		let result = parse_path("/Groups");
		assert!(matches!(result, Some(ResourcePath::Groups)));
	}

	#[test]
	fn test_parse_path_groups_id() {
		let result = parse_path("/Groups/456");
		assert!(matches!(result, Some(ResourcePath::GroupsId(ref id)) if id == "456"));
	}

	#[test]
	fn test_parse_path_invalid() {
		let result = parse_path("/Invalid");
		assert!(result.is_none());
	}

	#[test]
	fn test_resolve_bulk_id_refs_string() {
		let mut bulk_id_map = HashMap::new();
		bulk_id_map.insert("user1".to_string(), "uuid-12345".to_string());

		let mut value = serde_json::json!("bulkId:user1");
		resolve_bulk_id_refs(&mut value, &bulk_id_map);
		assert_eq!(value.as_str().unwrap(), "uuid-12345");
	}

	#[test]
	fn test_resolve_bulk_id_refs_nested() {
		let mut bulk_id_map = HashMap::new();
		bulk_id_map.insert("user1".to_string(), "uuid-12345".to_string());

		let mut value = serde_json::json!({
			"members": [
				{"value": "bulkId:user1"}
			]
		});
		resolve_bulk_id_refs(&mut value, &bulk_id_map);
		assert_eq!(value["members"][0]["value"].as_str().unwrap(), "uuid-12345");
	}

	#[test]
	fn test_resolve_bulk_id_refs_no_match() {
		let bulk_id_map = HashMap::new();

		let mut value = serde_json::json!("bulkId:unknown");
		resolve_bulk_id_refs(&mut value, &bulk_id_map);
		assert_eq!(value.as_str().unwrap(), "bulkId:unknown");
	}

	#[test]
	fn test_resolve_bulk_id_refs_non_bulk_id() {
		let bulk_id_map = HashMap::new();

		let mut value = serde_json::json!("regular-value");
		resolve_bulk_id_refs(&mut value, &bulk_id_map);
		assert_eq!(value.as_str().unwrap(), "regular-value");
	}
}
