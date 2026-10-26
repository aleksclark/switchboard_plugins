mod devices;
mod reading;
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
    lms_base_url: String,
}

#[no_mangle]
pub extern "C" fn name() -> u64 {
    sdk::leaked_string("primer")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&plugin_metadata())
}

fn plugin_metadata() -> sdk::PluginMetadata {
    sdk::PluginMetadata {
        name: "primer".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        abi_version: 1,
        description: "Administer Primer TV, Primer Tasks, the Primer reading library and reading plans, and Primer content ingest".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec![
            "tv_base_url".into(),
            "tv_admin_key".into(),
            "tasks_base_url".into(),
            "tasks_api_key".into(),
            "lms_base_url".into(),
            "oauth_issuer".into(),
            "oauth_client_id".into(),
            "oauth_scopes".into(),
            "oauth_token_key".into(),
            "oauth_subject".into(),
            "oauth_email".into(),
        ],
        plain_text_keys: vec![
            "tv_base_url".into(),
            "tasks_base_url".into(),
            "lms_base_url".into(),
            "oauth_issuer".into(),
            "oauth_client_id".into(),
            "oauth_scopes".into(),
            "oauth_token_key".into(),
            "oauth_subject".into(),
            "oauth_email".into(),
        ],
        optional_keys: vec![
            "tasks_api_key".into(),
            "lms_base_url".into(),
            "oauth_issuer".into(),
            "oauth_client_id".into(),
            "oauth_scopes".into(),
            "oauth_token_key".into(),
            "oauth_subject".into(),
            "oauth_email".into(),
        ],
        placeholders: HashMap::from([
            ("tv_base_url".into(), "http://primer-tv:8081/api/v1".into()),
            ("tv_admin_key".into(), "Primer TV admin API key".into()),
            (
                "tasks_base_url".into(),
                "http://primer-tasks:8082/tasks/api".into(),
            ),
            (
                "tasks_api_key".into(),
                "Leave blank for native OAuth; otherwise use a Tasks service API key".into(),
            ),
            (
                "lms_base_url".into(),
                "https://api.primerlms.com/api/v1".into(),
            ),
            ("oauth_issuer".into(), "https://clerk.primerlms.com".into()),
            (
                "oauth_client_id".into(),
                "Registered public Clerk OAuth client ID".into(),
            ),
            (
                "oauth_scopes".into(),
                "openid profile email offline_access tasks:read tasks:write reading:read reading:write"
                    .into(),
            ),
            ("oauth_token_key".into(), "tasks_api_key".into()),
            (
                "oauth_subject".into(),
                "Expected Clerk user ID (user_...)".into(),
            ),
            ("oauth_email".into(), "Expected Clerk account email".into()),
        ]),
    }
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
    let lms_base_url = credential(&credentials, "lms_base_url");
    if !lms_base_url.is_empty()
        && !lms_base_url.starts_with("https://")
        && !lms_base_url.starts_with("http://")
    {
        return sdk::leaked_string("primer: lms_base_url must be an http(s) URL");
    }
    *CONFIG.lock().unwrap() = Some(Config {
        tv_base_url: tv_base_url.trim_end_matches('/').into(),
        tv_admin_key,
        tasks_base_url: tasks_base_url.trim_end_matches('/').into(),
        tasks_api_key,
        lms_base_url: lms_base_url.trim_end_matches('/').into(),
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
    Lms,
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
        "primer_list_tv_devices" | "primer_get_tv_device" => {
            devices::execute(name == "primer_get_tv_device", &args, |path| {
                request(Service::Tv, "GET", path, String::new())
            })
        }
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
        reading::IMPORT_TOOL => import_epub(&args),
        _ => match reading::find(name) {
            Some(route) => reading_request(route, &args),
            None => sdk::err_result(&format!("unknown tool: {name}")),
        },
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
                    _ => reading::api_query_key(key),
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
    let value = match args.get(key) {
        Some(Value::Number(number)) => number.to_string(),
        _ => sdk::arg_str(args, key),
    };
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

enum Payload {
    Text(String),
    Bytes(Vec<u8>),
}

fn request(service: Service, method: &str, path: &str, body: String) -> Result<String, String> {
    request_with(service, method, path, Payload::Text(body), Vec::new())
}

fn request_with(
    service: Service,
    method: &str,
    path: &str,
    body: Payload,
    extra_headers: Vec<(&str, String)>,
) -> Result<String, String> {
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
        Service::Lms => {
            if config.lms_base_url.is_empty() {
                return Err("primer: lms_base_url is not configured; set it to the LMS API base including /api/v1".into());
            }
            (
                &config.lms_base_url,
                "Authorization",
                format!("Bearer {}", config.tasks_api_key),
            )
        }
    };
    let mut headers = HashMap::from([
        ("Content-Type".to_string(), "application/json".to_string()),
        (auth_header.to_string(), auth_value),
    ]);
    for (key, value) in extra_headers {
        headers.insert(key.into(), value);
    }
    let url = format!("{base_url}{path}");
    let request = match body {
        Payload::Text(body) => sdk::HttpRequest {
            method: method.into(),
            url,
            headers,
            body,
            body_base64: String::new(),
        },
        Payload::Bytes(bytes) => sdk::HttpRequest::with_body_bytes(method, &url, headers, &bytes),
    };
    drop(guard);
    let response = sdk::host_http_request(&request)?;
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

fn reading_request(route: &reading::Route, args: &HashMap<String, Value>) -> sdk::ToolResult {
    let mut path = String::new();
    for part in route.path.split('/').skip(1) {
        path.push('/');
        match part
            .strip_prefix('{')
            .and_then(|part| part.strip_suffix('}'))
        {
            Some(name) => match require_arg(args, name) {
                Ok(value) => path.push_str(&percent_encode(&value)),
                Err(error) => return error,
            },
            None => path.push_str(part),
        }
    }
    if route.method == "DELETE" && route.query.contains(&"version") {
        if let Err(error) = require_arg(args, "version") {
            return error;
        }
    }
    let body = match route.body {
        reading::Body::None => String::new(),
        reading::Body::Json(_) => match json_arg(args, "body") {
            Ok(value) => value,
            Err(error) => return error,
        },
    };
    let mut headers = Vec::new();
    if route.idempotent {
        match reading::idempotency_key(args, route.name, &path, &body) {
            Ok(key) => headers.push(("Idempotency-Key", key)),
            Err(error) => return error,
        }
    }
    let full_path = query_path(&path, args, route.query);
    match request_with(
        Service::Lms,
        route.method,
        &full_path,
        Payload::Text(body),
        headers,
    ) {
        Ok(value) => sdk::raw_result(value),
        Err(error) => sdk::err_result(&error),
    }
}

fn import_epub(args: &HashMap<String, Value>) -> sdk::ToolResult {
    let import = match reading::epub_import(args) {
        Ok(value) => value,
        Err(error) => return sdk::err_result(&error),
    };
    let download = sdk::host_http_request_raw_body(&sdk::HttpRequest {
        method: "GET".into(),
        url: import.source_url.clone(),
        headers: HashMap::from([
            (
                "Accept".into(),
                "application/epub+zip, application/octet-stream".into(),
            ),
            ("X-Host-Timeout-Seconds".into(), "120".into()),
        ]),
        body: String::new(),
        body_base64: String::new(),
    });
    let response = match download {
        Ok(response) => response,
        Err(_) => return sdk::err_result("EPUB download failed; check source_url"),
    };
    if !(200..300).contains(&response.status) {
        return sdk::err_result(&format!(
            "EPUB download failed with HTTP {}",
            response.status
        ));
    }
    let bytes = match response.body_bytes() {
        Ok(bytes) => bytes,
        Err(_) => return sdk::err_result("EPUB download could not be decoded"),
    };
    if let Err(error) = reading::validate_epub(&bytes) {
        return sdk::err_result(&error);
    }
    let digest = reading::sha256_hex(&bytes);
    let key = match reading::idempotency_key(args, reading::IMPORT_TOOL, &import.path, &digest) {
        Ok(key) => key,
        Err(error) => return error,
    };
    let headers = vec![
        ("Content-Type", "application/epub+zip".to_string()),
        ("Idempotency-Key", key),
        ("X-Content-SHA256", digest),
        ("X-Content-Length", bytes.len().to_string()),
        ("X-Rights-Source", import.rights_source),
        ("X-Rights-License", import.rights_license),
        (
            "X-Rights-Acknowledged",
            import.rights_acknowledged.to_string(),
        ),
        ("X-Host-Timeout-Seconds", "120".to_string()),
    ];
    match request_with(
        Service::Lms,
        "POST",
        &import.path,
        Payload::Bytes(bytes),
        headers,
    ) {
        Ok(value) => sdk::raw_result(value),
        Err(error) => sdk::err_result(&error),
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
    fn metadata_exposes_native_oauth_setup_without_token_storage() {
        let meta = plugin_metadata();
        for key in [
            "oauth_issuer",
            "oauth_client_id",
            "oauth_scopes",
            "oauth_token_key",
            "oauth_subject",
            "oauth_email",
        ] {
            assert!(meta.credential_keys.contains(&key.to_string()));
            assert!(meta.plain_text_keys.contains(&key.to_string()));
            assert!(meta.optional_keys.contains(&key.to_string()));
            assert!(meta.placeholders.contains_key(key));
        }
        for key in [
            "oauth_access_token",
            "oauth_refresh_token",
            "oauth_client_secret",
        ] {
            assert!(!meta.credential_keys.contains(&key.to_string()));
        }
        for key in ["tasks_api_key", "tv_admin_key"] {
            assert!(!meta.plain_text_keys.contains(&key.to_string()));
        }
        assert!(meta.optional_keys.contains(&"tasks_api_key".to_string()));
        assert_eq!(meta.placeholders["oauth_token_key"], "tasks_api_key");
        for key in ["lms_base_url"] {
            assert!(meta.credential_keys.contains(&key.to_string()));
            assert!(meta.plain_text_keys.contains(&key.to_string()));
            assert!(meta.optional_keys.contains(&key.to_string()));
        }
        assert!(meta.placeholders["oauth_scopes"].contains("reading:read reading:write"));
        assert_eq!(meta.version, env!("CARGO_PKG_VERSION"));
    }

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
    fn tv_device_discovery_explains_assignment_selection() {
        let definitions = tools::tool_definitions();
        let list = definitions
            .iter()
            .find(|tool| tool.name == "primer_list_tv_devices")
            .expect("TV device list must be registered");
        for term in [
            "Start here",
            "device",
            "video_watch",
            "deviceId",
            "pairing codes",
        ] {
            assert!(
                list.description.contains(term),
                "missing discovery guidance: {term}"
            );
        }
        let get = definitions
            .iter()
            .find(|tool| tool.name == "primer_get_tv_device")
            .expect("selected TV device lookup must be registered");
        assert_eq!(get.required, vec!["id"]);
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
    fn reading_tools_expose_required_arguments() {
        let definitions = tools::tool_definitions();
        let find = |name: &str| {
            definitions
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("{name} must be registered"))
        };
        let create = find("primer_create_reading_work");
        assert_eq!(create.required, vec!["body"]);
        assert!(create.parameters.contains_key("idempotency_key"));
        let schedule = find("primer_schedule_reading_segment");
        assert_eq!(schedule.required, vec!["id", "segment_id", "body"]);
        assert!(!schedule.parameters.contains_key("idempotency_key"));
        let delete = find("primer_delete_reading_plan");
        assert_eq!(delete.required, vec!["id", "version"]);
        let plans = find("primer_list_reading_plans");
        assert!(plans.required.is_empty());
        assert!(plans.parameters.contains_key("student_id"));
        let import = find("primer_import_reading_epub");
        for key in [
            "work_id",
            "edition_id",
            "source_url",
            "rights_source",
            "rights_license",
            "rights_acknowledged",
        ] {
            assert!(import.required.contains(&key.to_string()), "{key}");
        }
        let args = HashMap::from([("student_id".into(), serde_json::json!("s 1"))]);
        assert_eq!(
            query_path("/reading/plans", &args, &["student_id", "limit"]),
            "/reading/plans?studentId=s%201"
        );
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(
            percent_encode("status:in progress"),
            "status%3Ain%20progress"
        );
    }
}
