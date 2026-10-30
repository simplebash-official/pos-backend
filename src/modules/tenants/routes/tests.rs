use super::secrets_match::secrets_match;
use super::shop_activities_response::ShopActivitiesResponse;
use super::shop_activity_item::ShopActivityItem;
use super::shop_login_item::ShopLoginItem;
use super::shop_logins_response::ShopLoginsResponse;
use super::shop_user_response::ShopUserResponse;
use crate::domain::users::Role;
use chrono::Utc;

#[test]
fn secrets_match_only_when_identical() {
    assert!(secrets_match(b"abcdef", b"abcdef"));
    assert!(!secrets_match(b"abcdef", b"abcdeg"));
    assert!(!secrets_match(b"abcde", b"abcdef"));
    assert!(!secrets_match(b"abcdefg", b"abcdef"));
    assert!(!secrets_match(b"", b"abcdef"));
}

#[test]
fn shop_user_response_serializes_to_camel_case() {
    let now = Utc::now();
    let user = ShopUserResponse {
        id: "usr_123".to_string(),
        key: "usr_123".to_string(),
        username: "cashier1".to_string(),
        name: "Alice Smith".to_string(),
        role: Role::Staff,
        is_active: true,
        employee_key: Some("emp_123".to_string()),
        created_at: now,
        updated_at: now,
        last_login_at: Some(now),
    };

    let val = serde_json::to_value(&user).unwrap();
    assert_eq!(val["id"], "usr_123");
    assert_eq!(val["key"], "usr_123");
    assert_eq!(val["username"], "cashier1");
    assert_eq!(val["name"], "Alice Smith");
    assert_eq!(val["role"], "staff");
    assert_eq!(val["isActive"], true);
    assert_eq!(val["employeeKey"], "emp_123");
    assert!(val.get("createdAt").is_some());
    assert!(val.get("updatedAt").is_some());
    assert!(val.get("lastLoginAt").is_some());
}

#[test]
fn shop_logins_response_serializes_correctly() {
    let now = Utc::now();
    let item = ShopLoginItem {
        id: "lgs_123".to_string(),
        key: "lgs_123".to_string(),
        user_key: "usr_123".to_string(),
        name: "Alice Smith".to_string(),
        username: "cashier1".to_string(),
        role: Role::Staff,
        ip_address: Some("127.0.0.1".to_string()),
        user_agent: Some("Mozilla/5.0".to_string()),
        logged_in_at: now,
    };
    let resp = ShopLoginsResponse {
        items: vec![item],
        total: 1,
        page: 1,
        limit: 25,
    };

    let val = serde_json::to_value(&resp).unwrap();
    assert_eq!(val["total"], 1);
    assert_eq!(val["page"], 1);
    assert_eq!(val["limit"], 25);
    assert_eq!(val["items"][0]["key"], "lgs_123");
    assert_eq!(val["items"][0]["ipAddress"], "127.0.0.1");
    assert_eq!(val["items"][0]["userAgent"], "Mozilla/5.0");
}

#[test]
fn shop_activities_response_serializes_correctly() {
    let now = Utc::now();
    let item = ShopActivityItem {
        id: "inv_123".to_string(),
        kind: "sale".to_string(),
        reference: "INV-0001".to_string(),
        cashier_name: Some("Alice Smith".to_string()),
        user_role: Some("staff".to_string()),
        amount_cents: Some(15000),
        description: "Completed sale via cash".to_string(),
        created_at: now,
    };
    let unassigned = ShopActivityItem {
        id: "rep_123".to_string(),
        kind: "repair".to_string(),
        reference: "REP-0001".to_string(),
        cashier_name: None,
        user_role: None,
        amount_cents: Some(5000),
        description: "Repair ticket for iPhone 13 (status: received)".to_string(),
        created_at: now,
    };
    let resp = ShopActivitiesResponse {
        items: vec![item, unassigned],
        total: 2,
        page: 1,
        limit: 25,
    };

    let val = serde_json::to_value(&resp).unwrap();
    assert_eq!(val["total"], 2);
    assert_eq!(val["items"][0]["kind"], "sale");
    assert_eq!(val["items"][0]["reference"], "INV-0001");
    assert_eq!(val["items"][0]["cashierName"], "Alice Smith");
    assert_eq!(val["items"][0]["amountCents"], 15000);
    assert_eq!(val["items"][1]["kind"], "repair");
    assert_eq!(val["items"][1]["reference"], "REP-0001");
    assert!(val["items"][1].get("cashierName").is_none());
}
