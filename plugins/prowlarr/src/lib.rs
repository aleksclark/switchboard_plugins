mod tools;

use serde_json;
use std::collections::HashMap;
use std::sync::Mutex;
use switchboard_guest_sdk as sdk;

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

struct Config {
    base_url: String,
    api_key: String,
}

fn with_config<F, R>(f: F) -> R
where
    F: FnOnce(&Config) -> R,
{
    let guard = CONFIG.lock().unwrap();
    f(guard.as_ref().expect("not configured"))
}

fn base_url() -> String {
    with_config(|c| c.base_url.clone())
}

fn api_key() -> String {
    with_config(|c| c.api_key.clone())
}

#[no_mangle]
pub extern "C" fn name() -> u64 {
    sdk::leaked_string("prowlarr")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&sdk::PluginMetadata {
        name: "prowlarr".into(),
        version: "0.1.0".into(),
        abi_version: 1,
        description: "Prowlarr indexer manager — manage indexers, search across all indexers, sync with Sonarr/Radarr/Lidarr/Readarr".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec!["api_key".into(), "base_url".into()],
        plain_text_keys: vec!["base_url".into()],
        optional_keys: vec![],
        placeholders: HashMap::from([
            ("api_key".into(), "your-prowlarr-api-key".into()),
            ("base_url".into(), "http://localhost:9696".into()),
        ]),
    })
}

#[no_mangle]
pub extern "C" fn tools() -> u64 {
    let defs = tools::tool_definitions();
    let data = serde_json::to_vec(&defs).unwrap_or_default();
    sdk::leaked_result(&data)
}

#[no_mangle]
pub extern "C" fn configure(ptr_size: u64) -> u64 {
    let input = sdk::read_input(ptr_size);
    let creds: HashMap<String, String> = match serde_json::from_slice(&input) {
        Ok(c) => c,
        Err(e) => return sdk::leaked_string(&format!("invalid credentials JSON: {e}")),
    };

    let key = creds.get("api_key").cloned().unwrap_or_default();
    if key.is_empty() {
        return sdk::leaked_string("prowlarr: api_key is required");
    }
    let bu = creds
        .get("base_url")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_default();
    if bu.is_empty() {
        return sdk::leaked_string("prowlarr: base_url is required");
    }

    *CONFIG.lock().unwrap() = Some(Config {
        base_url: bu,
        api_key: key,
    });
    0
}

#[no_mangle]
pub extern "C" fn execute(ptr_size: u64) -> u64 {
    let input = sdk::read_input(ptr_size);
    let req: sdk::ExecuteRequest = match serde_json::from_slice(&input) {
        Ok(r) => r,
        Err(e) => {
            let r = sdk::err_result(&format!("invalid request: {e}"));
            let data = serde_json::to_vec(&r).unwrap_or_default();
            return sdk::leaked_result(&data);
        }
    };

    let result = dispatch(&req.tool_name, req.args);
    let data = serde_json::to_vec(&result).unwrap_or_default();
    sdk::leaked_result(&data)
}

#[no_mangle]
pub extern "C" fn healthy() -> i32 {
    match do_get("/api/v1/system/status") {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

type HandlerFn = fn(HashMap<String, serde_json::Value>) -> sdk::ToolResult;

fn dispatch(tool_name: &str, args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let handler: Option<HandlerFn> = match tool_name {
        // Indexers
        "prowlarr_list_indexers" => Some(list_indexers),
        "prowlarr_get_indexer" => Some(get_indexer),
        "prowlarr_add_indexer" => Some(add_indexer),
        "prowlarr_update_indexer" => Some(update_indexer),
        "prowlarr_delete_indexer" => Some(delete_indexer),
        "prowlarr_test_indexer" => Some(test_indexer),
        "prowlarr_test_all_indexers" => Some(test_all_indexers),
        "prowlarr_get_indexer_schema" => Some(get_indexer_schema),
        // Search
        "prowlarr_search" => Some(search),
        "prowlarr_grab_release" => Some(grab_release),
        // Applications
        "prowlarr_list_applications" => Some(list_applications),
        "prowlarr_get_application" => Some(get_application),
        "prowlarr_add_application" => Some(add_application),
        "prowlarr_update_application" => Some(update_application),
        "prowlarr_delete_application" => Some(delete_application),
        "prowlarr_test_all_applications" => Some(test_all_applications),
        "prowlarr_get_application_schema" => Some(get_application_schema),
        // Indexer Proxies
        "prowlarr_list_indexer_proxies" => Some(list_indexer_proxies),
        // System
        "prowlarr_get_status" => Some(get_status),
        // Tags
        "prowlarr_list_tags" => Some(list_tags),
        "prowlarr_create_tag" => Some(create_tag),
        "prowlarr_delete_tag" => Some(delete_tag),
        // Download Clients
        "prowlarr_list_download_clients" => Some(list_download_clients),
        "prowlarr_get_download_client" => Some(get_download_client),
        // Notifications
        "prowlarr_list_notifications" => Some(list_notifications),
        _ => None,
    };

    match handler {
        Some(f) => f(args),
        None => sdk::err_result(&format!("unknown tool: {tool_name}")),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn auth_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("X-Api-Key".into(), api_key());
    h.insert("Content-Type".into(), "application/json".into());
    h
}

fn do_get(path: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "GET".into(),
        url: format!("{}{}", base_url(), path),
        headers: auth_headers(),
        body: String::new(),
        body_base64: String::new(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("prowlarr API error ({}): {}", resp.status, resp.body));
    }
    Ok(resp.body)
}

fn do_post(path: &str, body: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "POST".into(),
        url: format!("{}{}", base_url(), path),
        headers: auth_headers(),
        body: body.to_string(),
        body_base64: String::new(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("prowlarr API error ({}): {}", resp.status, resp.body));
    }
    if resp.body.is_empty() {
        return Ok(r#"{"status":"success"}"#.into());
    }
    Ok(resp.body)
}

fn do_put(path: &str, body: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "PUT".into(),
        url: format!("{}{}", base_url(), path),
        headers: auth_headers(),
        body: body.to_string(),
        body_base64: String::new(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("prowlarr API error ({}): {}", resp.status, resp.body));
    }
    if resp.body.is_empty() {
        return Ok(r#"{"status":"success"}"#.into());
    }
    Ok(resp.body)
}

fn do_delete(path: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "DELETE".into(),
        url: format!("{}{}", base_url(), path),
        headers: auth_headers(),
        body: String::new(),
        body_base64: String::new(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("prowlarr API error ({}): {}", resp.status, resp.body));
    }
    if resp.body.is_empty() {
        return Ok(r#"{"status":"success"}"#.into());
    }
    Ok(resp.body)
}

fn require_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Result<String, sdk::ToolResult> {
    let v = sdk::arg_str(args, key);
    if v.is_empty() {
        return Err(sdk::err_result(&format!("{key} is required")));
    }
    Ok(v)
}

fn parse_json_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Result<Option<String>, sdk::ToolResult> {
    let v = sdk::arg_str(args, key);
    if v.is_empty() {
        return Ok(None);
    }
    let parsed: serde_json::Value = serde_json::from_str(&v)
        .map_err(|e| sdk::err_result(&format!("invalid JSON for {key}: {e}")))?;
    Ok(Some(serde_json::to_string(&parsed).unwrap()))
}

fn encode_path(s: &str) -> String {
    percent_encode(s)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

fn build_query(params: &[(&str, String)]) -> String {
    let filtered: Vec<String> = params.iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", k, encode_path(v)))
        .collect();
    if filtered.is_empty() { String::new() } else { format!("?{}", filtered.join("&")) }
}

// ── Indexers ────────────────────────────────────────────────────────

fn list_indexers(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/indexer") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_indexer(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/v1/indexer/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn add_indexer(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v1/indexer", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn update_indexer(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_put(&format!("/api/v1/indexer/{}", encode_path(&id)), &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_indexer(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("/api/v1/indexer/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn test_indexer(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v1/indexer/test", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn test_all_indexers(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_post("/api/v1/indexer/testall", "null") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_indexer_schema(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/indexer/schema") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Search ──────────────────────────────────────────────────────────

fn search(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let query = match require_arg(&args, "query") { Ok(v) => v, Err(r) => return r };
    let indexer_ids = sdk::arg_str(&args, "indexer_ids");
    let categories = sdk::arg_str(&args, "categories");
    let search_type = sdk::arg_str(&args, "type");
    let qs = build_query(&[
        ("query", query),
        ("indexerIds", indexer_ids),
        ("categories", categories),
        ("type", search_type),
    ]);
    match do_get(&format!("/api/v1/search{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn grab_release(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v1/search", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Applications ────────────────────────────────────────────────────

fn list_applications(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/applications") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_application(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/v1/applications/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn add_application(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v1/applications", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn update_application(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_put(&format!("/api/v1/applications/{}", encode_path(&id)), &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_application(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("/api/v1/applications/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn test_all_applications(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_post("/api/v1/applications/testall", "null") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_application_schema(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/applications/schema") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Indexer Proxies ─────────────────────────────────────────────────

fn list_indexer_proxies(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/indexerproxy") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── System ──────────────────────────────────────────────────────────

fn get_status(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/system/status") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Tags ────────────────────────────────────────────────────────────

fn list_tags(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/tag") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn create_tag(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v1/tag", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_tag(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("/api/v1/tag/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Download Clients ────────────────────────────────────────────────

fn list_download_clients(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/downloadclient") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_download_client(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/v1/downloadclient/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Notifications ───────────────────────────────────────────────────

fn list_notifications(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v1/notification") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}
