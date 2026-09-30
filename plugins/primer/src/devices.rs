use serde_json::{json, Value};
use std::collections::HashMap;
use switchboard_guest_sdk as sdk;

const RESPONSE_ERROR: &str = "Invalid TV device discovery response; response contents withheld";

pub(crate) fn execute(
    single: bool,
    args: &HashMap<String, Value>,
    fetch: impl FnOnce(&str) -> Result<String, String>,
) -> sdk::ToolResult {
    let path = match device_path(single, args) {
        Ok(path) => path,
        Err(error) => return sdk::err_result(error),
    };
    let body = match fetch(&path) {
        Ok(body) => body,
        Err(_) => return sdk::err_result(
            "TV device discovery request failed; check TV base URL and administrator authorization",
        ),
    };
    let projected = serde_json::from_str::<Value>(&body)
        .map_err(|_| RESPONSE_ERROR)
        .and_then(|value| {
            if single && value.get("id").and_then(Value::as_str) != path.strip_prefix("/devices/") {
                return Err(RESPONSE_ERROR);
            }
            project_response(value, single)
        });
    match projected {
        Ok(value) => sdk::raw_result(value.to_string()),
        Err(error) => sdk::err_result(error),
    }
}

fn valid_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
        && value != "00000000-0000-0000-0000-000000000000"
}

fn device_path(single: bool, args: &HashMap<String, Value>) -> Result<String, &'static str> {
    if single {
        let id = sdk::arg_str(args, "id");
        if !valid_id(&id) {
            return Err("id must be a TV device UUID from primer_list_tv_devices");
        }
        return Ok(format!("/devices/{}", id.to_ascii_lowercase()));
    }
    let mut validated = HashMap::new();
    for (key, default, minimum, maximum) in [("limit", 20, 1, 200), ("offset", 0, 0, 1_000_000)] {
        let number = match args.get(key) {
            None => default,
            Some(Value::Number(number)) => number.as_i64().ok_or("Invalid device pagination")?,
            Some(Value::String(value)) => value
                .parse::<i64>()
                .map_err(|_| "Invalid device pagination")?,
            _ => return Err("Invalid device pagination"),
        };
        if !(minimum..=maximum).contains(&number) {
            return Err("Device limit must be 1-200 and offset 0-1000000");
        }
        validated.insert(key.into(), json!(number));
    }
    for key in ["q", "sort", "dir", "filter"] {
        if let Some(value) = args.get(key) {
            let text = value
                .as_str()
                .ok_or("Device search parameters must be strings")?;
            let valid = match key {
                "q" => text.chars().count() <= 200,
                "sort" => [
                    "",
                    "name",
                    "kind",
                    "last_seen_at",
                    "paired_at",
                    "created_at",
                    "updated_at",
                ]
                .contains(&text),
                "dir" => ["", "asc", "desc"].contains(&text),
                "filter" => ["", "kind:tv_box", "kind:tablet"].contains(&text),
                _ => false,
            };
            if !valid {
                return Err("Unsupported device search, sort, direction or kind filter");
            }
            validated.insert(key.into(), value.clone());
        }
    }
    Ok(super::query_path(
        "/devices",
        &validated,
        &["limit", "offset", "q", "sort", "dir", "filter"],
    ))
}

fn project_response(value: Value, single: bool) -> Result<Value, &'static str> {
    if single {
        return project_device(&value);
    }
    let source = value.as_object().ok_or(RESPONSE_ERROR)?;
    let items = source
        .get("items")
        .and_then(Value::as_array)
        .ok_or(RESPONSE_ERROR)?;
    let total = source
        .get("totalCount")
        .and_then(Value::as_u64)
        .ok_or(RESPONSE_ERROR)?;
    let limit = source
        .get("limit")
        .and_then(Value::as_u64)
        .ok_or(RESPONSE_ERROR)?;
    let offset = source
        .get("offset")
        .and_then(Value::as_u64)
        .ok_or(RESPONSE_ERROR)?;
    if !(1..=200).contains(&limit) || items.len() as u64 > limit || items.len() as u64 > total {
        return Err(RESPONSE_ERROR);
    }
    let devices = items
        .iter()
        .map(project_device)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items": devices, "totalCount": total, "limit": limit, "offset": offset}))
}

fn project_device(value: &Value) -> Result<Value, &'static str> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or(RESPONSE_ERROR)?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or(RESPONSE_ERROR)?;
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(RESPONSE_ERROR)?;
    if !valid_id(id) || !["tv_box", "tablet"].contains(&kind) {
        return Err(RESPONSE_ERROR);
    }
    let mut output = json!({"id":id,"name":name,"kind":kind});
    for field in ["pairedAt", "revokedAt", "lastSeenAt"] {
        if let Some(value) = value.get(field) {
            if !value.is_null() && !value.is_string() {
                return Err(RESPONSE_ERROR);
            }
            output[field] = value.clone();
        }
    }
    let paired = value
        .get("pairedAt")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    let revoked = value.get("revokedAt").is_some_and(|value| !value.is_null());
    let no_pending_pairing = value.get("pairingCode").and_then(Value::as_str) == Some("");
    output["assignmentReady"] = json!(paired && !revoked && no_pending_pairing);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "4b06b1d8-944d-4668-a2e5-a682355ea541";

    fn device() -> Value {
        json!({"id": ID, "name": "Family TV", "kind": "tv_box", "pairedAt": "2026-09-01T12:00:00Z", "pairingCode": "", "tokenHash": "fixture-secret", "token": "fixture-secret", "futureCredential": {"secret": "fixture-secret"}})
    }

    #[test]
    fn list_preserves_paging_without_exposing_credentials() {
        let args = HashMap::from([
            ("limit".into(), json!(2)),
            ("offset".into(), json!(4)),
            ("q".into(), json!("Family & TV")),
            ("filter".into(), json!("kind:tv_box")),
        ]);
        let mut unpaired = device();
        unpaired["pairingCode"] = json!("fixture-pairing-code");
        let output = execute(false, &args, |path| {
            assert_eq!(
                path,
                "/devices?limit=2&offset=4&q=Family%20%26%20TV&filter=kind%3Atv_box"
            );
            Ok(json!({"items": [device(), unpaired], "totalCount": 9, "limit": 2, "offset": 4, "secret": "fixture-secret"}).to_string())
        });
        assert!(!output.is_error, "{}", output.data);
        assert!(!output.data.contains("fixture-secret"));
        assert!(!output.data.contains("fixture-pairing-code"));
        assert!(!output.data.contains("pairingCode"));
        let page: Value = serde_json::from_str(&output.data).unwrap();
        assert_eq!(page["totalCount"], 9);
        assert_eq!(page["offset"], 4);
        assert_eq!(page["limit"], 2);
        assert_eq!(page["items"][0]["assignmentReady"], true);
        assert_eq!(page["items"][1]["assignmentReady"], false);
    }

    #[test]
    fn get_reports_current_pairing_and_revocation_state() {
        for (paired, revoked, code, ready) in [
            (true, false, "", true),
            (false, false, "fixture-pairing-code", false),
            (true, true, "", false),
            (true, false, "fixture-pairing-code", false),
        ] {
            let mut record = device();
            record["pairedAt"] = if paired {
                json!("2026-09-01T12:00:00Z")
            } else {
                Value::Null
            };
            record["revokedAt"] = if revoked {
                json!("2026-09-02T12:00:00Z")
            } else {
                Value::Null
            };
            record["pairingCode"] = json!(code);
            let output = execute(true, &HashMap::from([("id".into(), json!(ID))]), |path| {
                assert_eq!(path, format!("/devices/{ID}"));
                Ok(record.to_string())
            });
            assert!(!output.is_error, "{}", output.data);
            let data: Value = serde_json::from_str(&output.data).unwrap();
            assert_eq!(data["id"], ID);
            assert_eq!(data["assignmentReady"], ready);
            assert!(!output.data.contains("fixture-secret"));
        }
    }

    #[test]
    fn validation_rejects_invalid_arguments_before_http() {
        for (single, args) in [
            (true, HashMap::new()),
            (
                true,
                HashMap::from([("id".into(), json!("../pairing-code"))]),
            ),
            (false, HashMap::from([("limit".into(), json!(0))])),
            (false, HashMap::from([("limit".into(), json!(201))])),
            (false, HashMap::from([("limit".into(), json!(1.5))])),
            (false, HashMap::from([("offset".into(), json!(-1))])),
            (false, HashMap::from([("offset".into(), json!(1000001))])),
            (
                false,
                HashMap::from([("q".into(), json!({"secret":"fixture-secret"}))]),
            ),
            (
                false,
                HashMap::from([("filter".into(), json!("pairing_code:fixture-secret"))]),
            ),
            (false, HashMap::from([("sort".into(), json!("token_hash"))])),
            (false, HashMap::from([("dir".into(), json!("sideways"))])),
        ] {
            let output = execute(single, &args, |_| panic!("invalid arguments reached HTTP"));
            assert!(output.is_error);
            assert!(!output.data.contains("fixture-secret"));
        }
    }

    #[test]
    fn empty_page_and_string_pagination_are_supported() {
        let args = HashMap::from([("limit".into(), json!("20")), ("offset".into(), json!("0"))]);
        let output = execute(false, &args, |path| {
            assert_eq!(path, "/devices?limit=20&offset=0");
            Ok(json!({"items":[],"totalCount":0,"limit":20,"offset":0}).to_string())
        });
        assert!(!output.is_error);
    }

    #[test]
    fn lookup_binds_identity_and_missing_pairing_state_is_not_ready() {
        let args = HashMap::from([("id".into(), json!(ID.to_ascii_uppercase()))]);
        let mut record = device();
        record.as_object_mut().unwrap().remove("pairingCode");
        let output = execute(true, &args, |path| {
            assert_eq!(path, format!("/devices/{ID}"));
            Ok(record.to_string())
        });
        assert!(!output.is_error);
        let projected: Value = serde_json::from_str(&output.data).unwrap();
        assert_eq!(projected["assignmentReady"], false);
        let mut wrong = device();
        wrong["id"] = json!("7b06b1d8-944d-4668-a2e5-a682355ea541");
        let output = execute(true, &args, |_| Ok(wrong.to_string()));
        assert!(output.is_error);
    }

    #[test]
    fn errors_and_malformed_responses_never_echo_private_data() {
        let args = HashMap::from([("id".into(), json!(ID))]);
        let output = execute(true, &args, |_| Err("HTTP 401 fixture-secret".into()));
        assert!(output.is_error);
        assert!(!output.data.contains("fixture-secret"));
        for body in [
            "fixture-secret".to_string(),
            json!({"items":null,"totalCount":0,"limit":20,"offset":0}).to_string(),
            json!({"items":[{"id":ID,"name":{"token":"fixture-secret"}}],"totalCount":1,"limit":20,"offset":0}).to_string(),
            json!({"items":[],"totalCount":"fixture-secret","limit":20,"offset":0}).to_string(),
        ] {
            let output = execute(false, &HashMap::new(), |_| Ok(body));
            assert!(output.is_error);
            assert!(!output.data.contains("fixture-secret"));
        }
    }
}
