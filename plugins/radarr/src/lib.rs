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
    sdk::leaked_string("radarr")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&sdk::PluginMetadata {
        name: "radarr".into(),
        version: "0.1.0".into(),
        abi_version: 1,
        description: "Radarr movie management — search, add, update, and manage movies, quality profiles, downloads, and more".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec!["api_key".into(), "base_url".into()],
        plain_text_keys: vec!["base_url".into()],
        optional_keys: vec![],
        placeholders: HashMap::from([
            ("api_key".into(), "your-radarr-api-key".into()),
            ("base_url".into(), "http://localhost:7878".into()),
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
        return sdk::leaked_string("radarr: api_key is required");
    }
    let bu = creds
        .get("base_url")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_default();
    if bu.is_empty() {
        return sdk::leaked_string("radarr: base_url is required");
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
    match do_get("/api/v3/system/status") {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

type HandlerFn = fn(HashMap<String, serde_json::Value>) -> sdk::ToolResult;

fn dispatch(tool_name: &str, args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let handler: Option<HandlerFn> = match tool_name {
        // Movies
        "radarr_list_movies" => Some(list_movies),
        "radarr_get_movie" => Some(get_movie),
        "radarr_add_movie" => Some(add_movie),
        "radarr_update_movie" => Some(update_movie),
        "radarr_delete_movie" => Some(delete_movie),
        "radarr_lookup_movie" => Some(lookup_movie),
        // Calendar
        "radarr_get_calendar" => Some(get_calendar),
        // Queue
        "radarr_get_queue" => Some(get_queue),
        "radarr_delete_queue_item" => Some(delete_queue_item),
        // History
        "radarr_get_history" => Some(get_history),
        // Commands
        "radarr_list_commands" => Some(list_commands),
        "radarr_run_command" => Some(run_command),
        "radarr_get_command" => Some(get_command),
        // System
        "radarr_get_status" => Some(get_status),
        // Quality Profiles
        "radarr_list_quality_profiles" => Some(list_quality_profiles),
        // Root Folders
        "radarr_list_root_folders" => Some(list_root_folders),
        // Tags
        "radarr_list_tags" => Some(list_tags),
        "radarr_create_tag" => Some(create_tag),
        "radarr_delete_tag" => Some(delete_tag),
        // Blocklist
        "radarr_get_blocklist" => Some(get_blocklist),
        "radarr_delete_blocklist_item" => Some(delete_blocklist_item),
        // Wanted/Missing
        "radarr_get_wanted_missing" => Some(get_wanted_missing),
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
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("radarr API error ({}): {}", resp.status, resp.body));
    }
    Ok(resp.body)
}

fn do_post(path: &str, body: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "POST".into(),
        url: format!("{}{}", base_url(), path),
        headers: auth_headers(),
        body: body.to_string(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("radarr API error ({}): {}", resp.status, resp.body));
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
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("radarr API error ({}): {}", resp.status, resp.body));
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
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("radarr API error ({}): {}", resp.status, resp.body));
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

// ── Movies ──────────────────────────────────────────────────────────

fn list_movies(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v3/movie") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_movie(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/v3/movie/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn add_movie(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v3/movie", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn update_movie(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_put(&format!("/api/v3/movie/{}", encode_path(&id)), &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_movie(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    let mut query_params: Vec<(&str, String)> = Vec::new();
    if sdk::arg_bool(&args, "delete_files") == Some(true) {
        query_params.push(("deleteFiles", "true".into()));
    }
    if sdk::arg_bool(&args, "add_import_exclusion") == Some(true) {
        query_params.push(("addImportExclusion", "true".into()));
    }
    let qs = build_query(&query_params);
    match do_delete(&format!("/api/v3/movie/{}{}", encode_path(&id), qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn lookup_movie(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let term = match require_arg(&args, "term") { Ok(v) => v, Err(r) => return r };
    let qs = build_query(&[("term", term)]);
    match do_get(&format!("/api/v3/movie/lookup{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Calendar ────────────────────────────────────────────────────────

fn get_calendar(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let start = sdk::arg_str(&args, "start");
    let end = sdk::arg_str(&args, "end");
    let qs = build_query(&[("start", start), ("end", end)]);
    match do_get(&format!("/api/v3/calendar{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Queue ───────────────────────────────────────────────────────────

fn get_queue(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let page = sdk::arg_str(&args, "page");
    let page_size = sdk::arg_str(&args, "page_size");
    let sort_key = sdk::arg_str(&args, "sort_key");
    let sort_direction = sdk::arg_str(&args, "sort_direction");
    let qs = build_query(&[
        ("page", page),
        ("pageSize", page_size),
        ("sortKey", sort_key),
        ("sortDirection", sort_direction),
    ]);
    match do_get(&format!("/api/v3/queue{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_queue_item(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    let mut query_params: Vec<(&str, String)> = Vec::new();
    if sdk::arg_bool(&args, "remove_from_client") == Some(true) {
        query_params.push(("removeFromClient", "true".into()));
    }
    if sdk::arg_bool(&args, "blocklist") == Some(true) {
        query_params.push(("blocklist", "true".into()));
    }
    let qs = build_query(&query_params);
    match do_delete(&format!("/api/v3/queue/{}{}", encode_path(&id), qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── History ─────────────────────────────────────────────────────────

fn get_history(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let page = sdk::arg_str(&args, "page");
    let page_size = sdk::arg_str(&args, "page_size");
    let sort_key = sdk::arg_str(&args, "sort_key");
    let sort_direction = sdk::arg_str(&args, "sort_direction");
    let event_type = sdk::arg_str(&args, "event_type");
    let qs = build_query(&[
        ("page", page),
        ("pageSize", page_size),
        ("sortKey", sort_key),
        ("sortDirection", sort_direction),
        ("eventType", event_type),
    ]);
    match do_get(&format!("/api/v3/history{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Commands ────────────────────────────────────────────────────────

fn list_commands(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v3/command") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn run_command(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let body = match parse_json_arg(&args, "body") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("body is required"),
        Err(r) => return r,
    };
    match do_post("/api/v3/command", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_command(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/v3/command/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── System ──────────────────────────────────────────────────────────

fn get_status(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v3/system/status") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Quality Profiles ────────────────────────────────────────────────

fn list_quality_profiles(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v3/qualityprofile") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Root Folders ────────────────────────────────────────────────────

fn list_root_folders(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v3/rootfolder") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Tags ────────────────────────────────────────────────────────────

fn list_tags(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/v3/tag") {
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
    match do_post("/api/v3/tag", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_tag(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("/api/v3/tag/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Blocklist ───────────────────────────────────────────────────────

fn get_blocklist(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let page = sdk::arg_str(&args, "page");
    let page_size = sdk::arg_str(&args, "page_size");
    let qs = build_query(&[("page", page), ("pageSize", page_size)]);
    match do_get(&format!("/api/v3/blocklist{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_blocklist_item(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "id") { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("/api/v3/blocklist/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ── Wanted/Missing ──────────────────────────────────────────────────

fn get_wanted_missing(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let page = sdk::arg_str(&args, "page");
    let page_size = sdk::arg_str(&args, "page_size");
    let sort_key = sdk::arg_str(&args, "sort_key");
    let sort_direction = sdk::arg_str(&args, "sort_direction");
    let qs = build_query(&[
        ("page", page),
        ("pageSize", page_size),
        ("sortKey", sort_key),
        ("sortDirection", sort_direction),
    ]);
    match do_get(&format!("/api/v3/wanted/missing{}", qs)) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}
