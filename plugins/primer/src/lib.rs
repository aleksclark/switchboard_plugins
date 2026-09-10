mod tools;

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use switchboard_guest_sdk as sdk;

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

struct Config {
    tv_base_url: String,
    tv_admin_key: String,
    tasks_base_url: String,
    tasks_api_key: String,
}

#[no_mangle]
pub extern "C" fn name() -> u64 {
    sdk::leaked_string("primer")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&sdk::PluginMetadata {
        name: "primer".into(),
        version: "0.1.0".into(),
        abi_version: 1,
        description: "Administer Primer TV, Primer Tasks, and Primer content ingest".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec![
            "tv_base_url".into(),
            "tv_admin_key".into(),
            "tasks_base_url".into(),
            "tasks_api_key".into(),
        ],
        plain_text_keys: vec!["tv_base_url".into(), "tasks_base_url".into()],
        optional_keys: vec![],
        placeholders: HashMap::from([
            ("tv_base_url".into(), "http://primer-tv:8081/api/v1".into()),
            ("tv_admin_key".into(), "Primer TV admin API key".into()),
            (
                "tasks_base_url".into(),
                "http://primer-tasks:8082/tasks/api".into(),
            ),
            (
                "tasks_api_key".into(),
                "Primer Tasks tenant-scoped service API key".into(),
            ),
        ]),
    })
}

#[no_mangle]
pub extern "C" fn tools() -> u64 {
    sdk::leaked_result(&serde_json::to_vec(&tools::tool_definitions()).unwrap_or_default())
}

#[no_mangle]
pub extern "C" fn configure(ptr_size: u64) -> u64 {
    let input = sdk::read_input(ptr_size);
    let credentials: HashMap<String, String> = match serde_json::from_slice(&input) {
        Ok(value) => value,
        Err(error) => return sdk::leaked_string(&format!("invalid credentials JSON: {error}")),
    };
    let tv_base_url = credential(&credentials, "tv_base_url");
    if tv_base_url.is_empty() {
        return sdk::leaked_string("primer: tv_base_url is required");
    }
    let tv_admin_key = credential(&credentials, "tv_admin_key");
    if tv_admin_key.is_empty() {
        return sdk::leaked_string("primer: tv_admin_key is required");
    }
    let tasks_base_url = credential(&credentials, "tasks_base_url");
    if tasks_base_url.is_empty() {
        return sdk::leaked_string("primer: tasks_base_url is required");
    }
    let tasks_api_key = credential(&credentials, "tasks_api_key");
    if tasks_api_key.is_empty() {
        return sdk::leaked_string("primer: tasks_api_key is required");
    }
    *CONFIG.lock().unwrap() = Some(Config {
        tv_base_url: tv_base_url.trim_end_matches('/').into(),
        tv_admin_key,
        tasks_base_url: tasks_base_url.trim_end_matches('/').into(),
        tasks_api_key,
    });
    0
}

#[no_mangle]
pub extern "C" fn execute(ptr_size: u64) -> u64 {
    let input = sdk::read_input(ptr_size);
    let request: sdk::ExecuteRequest = match serde_json::from_slice(&input) {
        Ok(value) => value,
        Err(error) => return result(sdk::err_result(&format!("invalid request: {error}"))),
    };
    result(dispatch(&request.tool_name, request.args))
}

#[no_mangle]
pub extern "C" fn healthy() -> i32 {
    if request(Service::Tv, "GET", "/health", String::new()).is_ok()
        && request(Service::Tasks, "GET", "/students?limit=1", String::new()).is_ok()
    {
        1
    } else {
        0
    }
}

enum Service {
    Tv,
    Tasks,
}

fn dispatch(name: &str, args: HashMap<String, Value>) -> sdk::ToolResult {
    match name {
        "primer_list_media_items" => list(Service::Tv, "/media-items", &args),
        "primer_get_media_item" => get(Service::Tv, "/media-items", &args),
        "primer_create_media_item" => body_request(Service::Tv, "POST", "/media-items", &args),
        "primer_update_media_item" => id_body_request(Service::Tv, "PATCH", "/media-items", &args),
        "primer_ingest_media_item" => {
            id_body_suffix_request(Service::Tv, "POST", "/media-items", "/ingest", &args)
        }
        "primer_delete_media_item" => id_request(Service::Tv, "DELETE", "/media-items", &args),
        "primer_get_schedule_grid" => query_request(
            Service::Tv,
            "GET",
            "/schedule-grid",
            &args,
            &["from", "days"],
        ),
        "primer_list_schedule_entries" => list(Service::Tv, "/schedule-entries", &args),
        "primer_create_schedule_entry" => {
            body_request(Service::Tv, "POST", "/schedule-entries", &args)
        }
        "primer_update_schedule_entry" => {
            id_body_request(Service::Tv, "PATCH", "/schedule-entries", &args)
        }
        "primer_delete_schedule_entry" => {
            id_request(Service::Tv, "DELETE", "/schedule-entries", &args)
        }
        "primer_list_tv_devices" => list(Service::Tv, "/devices", &args),
        "primer_list_content_manifest_entries" => {
            list(Service::Tv, "/content-manifest-entries", &args)
        }
        "primer_sync_content_manifest" => {
            body_request(Service::Tv, "POST", "/content-manifest/sync", &args)
        }
        "primer_record_content_attempt" => content_attempt(&args),
        "primer_mark_content_present" => slug_request("/present", &args, "{}".into()),
        "primer_browse_jellyfin" => query_request(
            Service::Tv,
            "GET",
            "/jellyfin/browse",
            &args,
            &["parent_id", "q", "limit", "start_index"],
        ),
        "primer_sync_jellyfin" => {
            request_result(Service::Tv, "POST", "/jellyfin/sync", "{}".into())
        }
        "primer_get_rotation_suggestions" => query_request(
            Service::Tv,
            "GET",
            "/rotation/suggestions",
            &args,
            &["limit"],
        ),
        "primer_rotate_content" => body_request(Service::Tv, "POST", "/rotation/rotate", &args),
        "primer_get_tv_metrics" => query_request(Service::Tv, "GET", "/metrics", &args, &["days"]),
        "primer_list_reports" => list(Service::Tv, "/primer-reports", &args),
        "primer_run_reports" => {
            request_result(Service::Tv, "POST", "/primer-reports/run", "{}".into())
        }
        "primer_list_students" => list(Service::Tasks, "/students", &args),
        "primer_list_tasks" => query_request(
            Service::Tasks,
            "GET",
            "/tasks",
            &args,
            &["q", "limit", "offset", "sort", "dir", "status", "view"],
        ),
        "primer_create_task" => body_request(Service::Tasks, "POST", "/tasks", &args),
        "primer_revise_task" => {
            id_body_suffix_request(Service::Tasks, "POST", "/tasks", "/revisions", &args)
        }
        "primer_publish_task" => {
            id_suffix_request(Service::Tasks, "POST", "/tasks", "/publish", &args)
        }
        "primer_retire_task" => {
            id_suffix_request(Service::Tasks, "POST", "/tasks", "/retire", &args)
        }
        "primer_list_task_schedules" => list(Service::Tasks, "/schedules", &args),
        "primer_create_task_schedule" => body_request(Service::Tasks, "POST", "/schedules", &args),
        "primer_update_task_schedule" => {
            id_body_request(Service::Tasks, "PATCH", "/schedules", &args)
        }
        "primer_delete_task_schedule" => id_request(Service::Tasks, "DELETE", "/schedules", &args),
        "primer_list_occurrences" => list(Service::Tasks, "/occurrences", &args),
        "primer_get_occurrence" => get(Service::Tasks, "/occurrences", &args),
        "primer_decide_occurrence" => {
            id_body_suffix_request(Service::Tasks, "POST", "/occurrences", "/decision", &args)
        }
        "primer_retry_occurrence" => occurrence_retry(&args),
        "primer_skip_occurrence" => {
            id_suffix_request(Service::Tasks, "POST", "/occurrences", "/skip", &args)
        }
        "primer_cancel_occurrence" => {
            id_suffix_request(Service::Tasks, "POST", "/occurrences", "/cancel", &args)
        }
        _ => sdk::err_result(&format!("unknown tool: {name}")),
    }
}

fn credential(credentials: &HashMap<String, String>, key: &str) -> String {
    credentials.get(key).cloned().unwrap_or_default()
}

fn list(service: Service, path: &str, args: &HashMap<String, Value>) -> sdk::ToolResult {
    query_request(
        service,
        "GET",
        path,
        args,
        &["limit", "offset", "q", "sort", "dir", "filter"],
    )
}

fn get(service: Service, prefix: &str, args: &HashMap<String, Value>) -> sdk::ToolResult {
    id_request(service, "GET", prefix, args)
}

fn id_request(
    service: Service,
    method: &str,
    prefix: &str,
    args: &HashMap<String, Value>,
) -> sdk::ToolResult {
    let id = match require_arg(args, "id") {
        Ok(value) => value,
        Err(error) => return error,
    };
    request_result(
        service,
        method,
        &format!("{prefix}/{}", percent_encode(&id)),
        String::new(),
    )
}

fn id_suffix_request(
    service: Service,
    method: &str,
    prefix: &str,
    suffix: &str,
    args: &HashMap<String, Value>,
) -> sdk::ToolResult {
    let id = match require_arg(args, "id") {
        Ok(value) => value,
        Err(error) => return error,
    };
    request_result(
        service,
        method,
        &format!("{prefix}/{}{suffix}", percent_encode(&id)),
        "{}".into(),
    )
}

fn body_request(
    service: Service,
    method: &str,
    path: &str,
    args: &HashMap<String, Value>,
) -> sdk::ToolResult {
    let body = match json_arg(args, "body") {
        Ok(value) => value,
        Err(error) => return error,
    };
    request_result(service, method, path, body)
}

fn id_body_request(
    service: Service,
    method: &str,
    prefix: &str,
    args: &HashMap<String, Value>,
) -> sdk::ToolResult {
    id_body_suffix_request(service, method, prefix, "", args)
}

fn id_body_suffix_request(
    service: Service,
    method: &str,
    prefix: &str,
    suffix: &str,
    args: &HashMap<String, Value>,
) -> sdk::ToolResult {
    let id = match require_arg(args, "id") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let body = match json_arg(args, "body") {
        Ok(value) => value,
        Err(error) => return error,
    };
    request_result(
        service,
        method,
        &format!("{prefix}/{}{suffix}", percent_encode(&id)),
        body,
    )
}

fn query_request(
    service: Service,
    method: &str,
    path: &str,
    args: &HashMap<String, Value>,
    keys: &[&str],
) -> sdk::ToolResult {
    let full_path = query_path(path, args, keys);
    request_result(service, method, &full_path, String::new())
}

fn query_path(path: &str, args: &HashMap<String, Value>, keys: &[&str]) -> String {
    let query = keys
        .iter()
        .filter_map(|key| {
            let value = match args.get(*key) {
                Some(Value::Number(_)) => sdk::arg_int(args, key)
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                _ => sdk::arg_str(args, key),
            };
            if value.is_empty() {
                None
            } else {
                let api_key = match *key {
                    "parent_id" => "parentId",
                    "start_index" => "startIndex",
                    "requirement_id" => "requirementId",
                    "attempt_id" => "attemptId",
                    _ => key,
                };
                Some(format!("{api_key}={}", percent_encode(&value)))
            }
        })
        .collect::<Vec<_>>();
    if query.is_empty() {
        path.into()
    } else {
        format!("{path}?{}", query.join("&"))
    }
}

fn content_attempt(args: &HashMap<String, Value>) -> sdk::ToolResult {
    let error = sdk::arg_str(args, "error");
    let body = if error.is_empty() {
        "{}".into()
    } else {
        serde_json::json!({"error": error}).to_string()
    };
    slug_request("/attempt", args, body)
}

fn slug_request(suffix: &str, args: &HashMap<String, Value>, body: String) -> sdk::ToolResult {
    let slug = match require_arg(args, "slug") {
        Ok(value) => value,
        Err(error) => return error,
    };
    request_result(
        Service::Tv,
        "POST",
        &format!(
            "/content-manifest-entries/{}{suffix}",
            percent_encode(&slug)
        ),
        body,
    )
}

fn occurrence_retry(args: &HashMap<String, Value>) -> sdk::ToolResult {
    let id = match require_arg(args, "id") {
        Ok(value) => value,
        Err(error) => return error,
    };
    query_request(
        Service::Tasks,
        "POST",
        &format!("/occurrences/{}/retry", percent_encode(&id)),
        args,
        &["requirement_id", "attempt_id"],
    )
}

fn require_arg(args: &HashMap<String, Value>, key: &str) -> Result<String, sdk::ToolResult> {
    let value = sdk::arg_str(args, key);
    if value.is_empty() {
        Err(sdk::err_result(&format!("{key} is required")))
    } else {
        Ok(value)
    }
}

fn json_arg(args: &HashMap<String, Value>, key: &str) -> Result<String, sdk::ToolResult> {
    let value = require_arg(args, key)?;
    let parsed: Value = serde_json::from_str(&value)
        .map_err(|error| sdk::err_result(&format!("invalid JSON for {key}: {error}")))?;
    serde_json::to_string(&parsed)
        .map_err(|error| sdk::err_result(&format!("invalid JSON for {key}: {error}")))
}

fn request_result(service: Service, method: &str, path: &str, body: String) -> sdk::ToolResult {
    match request(service, method, path, body) {
        Ok(value) => sdk::raw_result(value),
        Err(error) => sdk::err_result(&error),
    }
}

fn request(service: Service, method: &str, path: &str, body: String) -> Result<String, String> {
    let guard = CONFIG
        .lock()
        .map_err(|_| "primer configuration lock failed".to_string())?;
    let config = guard
        .as_ref()
        .ok_or_else(|| "primer is not configured".to_string())?;
    let (base_url, auth_header, auth_value) = match service {
        Service::Tv => (
            &config.tv_base_url,
            "X-Admin-Key",
            config.tv_admin_key.clone(),
        ),
        Service::Tasks => (
            &config.tasks_base_url,
            "Authorization",
            format!("Bearer {}", config.tasks_api_key),
        ),
    };
    let response = sdk::host_http_request(&sdk::HttpRequest {
        method: method.into(),
        url: format!("{base_url}{path}"),
        headers: HashMap::from([
            ("Content-Type".into(), "application/json".into()),
            (auth_header.into(), auth_value),
        ]),
        body,
        body_base64: String::new(),
    })?;
    if response.status >= 400 {
        return Err(format!(
            "Primer API error ({}): {}",
            response.status, response.body
        ));
    }
    if response.body.is_empty() {
        Ok(r#"{"status":"success"}"#.into())
    } else {
        Ok(response.body)
    }
}

fn result(value: sdk::ToolResult) -> u64 {
    sdk::leaked_result(&serde_json::to_vec(&value).unwrap_or_default())
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_unique_and_discoverable() {
        let definitions = tools::tool_definitions();
        let mut names = definitions
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), definitions.len());
        assert!(definitions
            .iter()
            .all(|tool| tool.name.starts_with("primer_")));
        assert!(definitions
            .iter()
            .any(|tool| tool.description.contains("Start here")));
    }

    #[test]
    fn preserves_numeric_pagination_and_encodes_filters() {
        let args = HashMap::from([
            ("limit".into(), serde_json::json!(1)),
            ("offset".into(), serde_json::json!(0)),
            ("q".into(), serde_json::json!("read & learn")),
        ]);
        assert_eq!(
            query_path("/tasks", &args, &["limit", "offset", "q"]),
            "/tasks?limit=1&offset=0&q=read%20%26%20learn"
        );
        let args = HashMap::from([("limit".into(), serde_json::json!("2"))]);
        assert_eq!(query_path("/tasks", &args, &["limit"]), "/tasks?limit=2");
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(
            percent_encode("status:in progress"),
            "status%3Ain%20progress"
        );
    }
}
