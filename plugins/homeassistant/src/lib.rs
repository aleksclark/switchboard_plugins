mod tools;

use serde_json;
use std::collections::HashMap;
use std::sync::Mutex;
use switchboard_guest_sdk as sdk;

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

struct Config {
    base_url: String,
    token: String,
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

fn token() -> String {
    with_config(|c| c.token.clone())
}

#[no_mangle]
pub extern "C" fn name() -> u64 {
    sdk::leaked_string("homeassistant")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&sdk::PluginMetadata {
        name: "homeassistant".into(),
        version: "0.1.0".into(),
        abi_version: 1,
        description: "Home Assistant smart home integration — control lights, sensors, automations, scenes, scripts, and more".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec!["token".into(), "base_url".into()],
        plain_text_keys: vec!["base_url".into()],
        optional_keys: vec![],
        placeholders: HashMap::from([
            ("token".into(), "your-long-lived-access-token".into()),
            ("base_url".into(), "http://homeassistant.local:8123".into()),
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

    let tok = creds.get("token").cloned().unwrap_or_default();
    if tok.is_empty() {
        return sdk::leaked_string("homeassistant: token is required");
    }
    let bu = creds
        .get("base_url")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_default();
    if bu.is_empty() {
        return sdk::leaked_string("homeassistant: base_url is required");
    }

    *CONFIG.lock().unwrap() = Some(Config {
        base_url: bu,
        token: tok,
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
    match do_get("/api/") {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

type HandlerFn = fn(HashMap<String, serde_json::Value>) -> sdk::ToolResult;

fn dispatch(tool_name: &str, args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let handler: Option<HandlerFn> = match tool_name {
        "homeassistant_list_states" => Some(list_states),
        "homeassistant_get_state" => Some(get_state),
        "homeassistant_set_state" => Some(set_state),
        "homeassistant_delete_state" => Some(delete_state),
        "homeassistant_list_services" => Some(list_services),
        "homeassistant_call_service" => Some(call_service),
        "homeassistant_list_events" => Some(list_events),
        "homeassistant_fire_event" => Some(fire_event),
        "homeassistant_get_history" => Some(get_history),
        "homeassistant_get_logbook" => Some(get_logbook),
        "homeassistant_get_config" => Some(get_config),
        "homeassistant_check_config" => Some(check_config),
        "homeassistant_render_template" => Some(render_template),
        "homeassistant_get_error_log" => Some(get_error_log),
        "homeassistant_list_calendars" => Some(list_calendars),
        "homeassistant_get_calendar_events" => Some(get_calendar_events),
        "homeassistant_handle_intent" => Some(handle_intent),
        "homeassistant_get_automation" => Some(get_automation),
        "homeassistant_save_automation" => Some(save_automation),
        "homeassistant_delete_automation" => Some(delete_automation),
        "homeassistant_get_scene" => Some(get_scene),
        "homeassistant_save_scene" => Some(save_scene),
        "homeassistant_delete_scene" => Some(delete_scene),
        "homeassistant_get_script" => Some(get_script),
        "homeassistant_save_script" => Some(save_script),
        "homeassistant_delete_script" => Some(delete_script),
        _ => None,
    };

    match handler {
        Some(f) => f(args),
        None => sdk::err_result(&format!("unknown tool: {tool_name}")),
    }
}

fn auth_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Authorization".into(), format!("Bearer {}", token()));
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
        return Err(format!("homeassistant API error ({}): {}", resp.status, resp.body));
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
        return Err(format!("homeassistant API error ({}): {}", resp.status, resp.body));
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
        return Err(format!("homeassistant API error ({}): {}", resp.status, resp.body));
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

fn list_states(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/states") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_state(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "entity_id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/states/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn set_state(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "entity_id") { Ok(v) => v, Err(r) => return r };
    let state = sdk::arg_str(&args, "state");
    let mut body = serde_json::json!({"state": state});
    if let Ok(Some(attrs)) = parse_json_arg(&args, "attributes") {
        let parsed: serde_json::Value = serde_json::from_str(&attrs).unwrap();
        body["attributes"] = parsed;
    } else if let Err(r) = parse_json_arg(&args, "attributes") {
        return r;
    }
    match do_post(&format!("/api/states/{}", encode_path(&id)), &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn delete_state(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "entity_id") { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("/api/states/{}", encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn list_services(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/services") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn call_service(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let domain = match require_arg(&args, "domain") { Ok(v) => v, Err(r) => return r };
    let service = match require_arg(&args, "service") { Ok(v) => v, Err(r) => return r };
    let body = match parse_json_arg(&args, "service_data") {
        Ok(Some(v)) => v,
        Ok(None) => "null".into(),
        Err(r) => return r,
    };
    let mut path = format!("/api/services/{}/{}", encode_path(&domain), encode_path(&service));
    let ret = sdk::arg_str(&args, "return_response");
    if ret == "true" {
        path.push_str("?return_response");
    }
    match do_post(&path, &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn list_events(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/events") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn fire_event(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let event_type = match require_arg(&args, "event_type") { Ok(v) => v, Err(r) => return r };
    let body = match parse_json_arg(&args, "event_data") {
        Ok(Some(v)) => v,
        Ok(None) => "null".into(),
        Err(r) => return r,
    };
    match do_post(&format!("/api/events/{}", encode_path(&event_type)), &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_history(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let entity_id = match require_arg(&args, "entity_id") { Ok(v) => v, Err(r) => return r };
    let mut path = "/api/history/period".to_string();
    let start = sdk::arg_str(&args, "start_time");
    if !start.is_empty() {
        path.push('/');
        path.push_str(&encode_path(&start));
    }
    let mut params = vec![format!("filter_entity_id={}", entity_id)];
    let end = sdk::arg_str(&args, "end_time");
    if !end.is_empty() {
        params.push(format!("end_time={}", end));
    }
    if sdk::arg_bool(&args, "minimal_response") == Some(true) {
        params.push("minimal_response".into());
    }
    if sdk::arg_bool(&args, "no_attributes") == Some(true) {
        params.push("no_attributes".into());
    }
    if sdk::arg_bool(&args, "significant_changes_only") == Some(true) {
        params.push("significant_changes_only".into());
    }
    path.push('?');
    path.push_str(&params.join("&"));
    match do_get(&path) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_logbook(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let mut path = "/api/logbook".to_string();
    let start = sdk::arg_str(&args, "start_time");
    if !start.is_empty() {
        path.push('/');
        path.push_str(&encode_path(&start));
    }
    let mut params = Vec::new();
    let entity = sdk::arg_str(&args, "entity_id");
    if !entity.is_empty() {
        params.push(format!("entity={}", entity));
    }
    let end = sdk::arg_str(&args, "end_time");
    if !end.is_empty() {
        params.push(format!("end_time={}", end));
    }
    if !params.is_empty() {
        path.push('?');
        path.push_str(&params.join("&"));
    }
    match do_get(&path) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_config(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/config") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn check_config(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_post("/api/config/core/check_config", "null") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn render_template(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let tmpl = match require_arg(&args, "template") { Ok(v) => v, Err(r) => return r };
    let body = serde_json::json!({"template": tmpl}).to_string();
    match do_post("/api/template", &body) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_error_log(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/error_log") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn list_calendars(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/calendars") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_calendar_events(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let entity_id = match require_arg(&args, "entity_id") { Ok(v) => v, Err(r) => return r };
    let start = match require_arg(&args, "start") { Ok(v) => v, Err(r) => return r };
    let end = match require_arg(&args, "end") { Ok(v) => v, Err(r) => return r };
    let path = format!("/api/calendars/{}?start={}&end={}", encode_path(&entity_id), start, end);
    match do_get(&path) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn handle_intent(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let intent_name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let mut body = serde_json::json!({"name": intent_name});
    if let Ok(Some(data)) = parse_json_arg(&args, "data") {
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        body["data"] = parsed;
    } else if let Err(r) = parse_json_arg(&args, "data") {
        return r;
    }
    match do_post("/api/intent/handle", &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn crud_get(args: HashMap<String, serde_json::Value>, id_key: &str, api_prefix: &str) -> sdk::ToolResult {
    let id = match require_arg(&args, id_key) { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("{}/{}", api_prefix, encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn crud_save(args: HashMap<String, serde_json::Value>, id_key: &str, api_prefix: &str) -> sdk::ToolResult {
    let id = match require_arg(&args, id_key) { Ok(v) => v, Err(r) => return r };
    let config = match require_arg(&args, "config") { Ok(v) => v, Err(r) => return r };
    let parsed: serde_json::Value = match serde_json::from_str(&config) {
        Ok(v) => v,
        Err(e) => return sdk::err_result(&format!("invalid JSON for config: {e}")),
    };
    match do_post(&format!("{}/{}", api_prefix, encode_path(&id)), &serde_json::to_string(&parsed).unwrap()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn crud_delete(args: HashMap<String, serde_json::Value>, id_key: &str, api_prefix: &str) -> sdk::ToolResult {
    let id = match require_arg(&args, id_key) { Ok(v) => v, Err(r) => return r };
    match do_delete(&format!("{}/{}", api_prefix, encode_path(&id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn get_automation(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_get(args, "automation_id", "/api/config/automation/config") }
fn save_automation(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_save(args, "automation_id", "/api/config/automation/config") }
fn delete_automation(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_delete(args, "automation_id", "/api/config/automation/config") }
fn get_scene(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_get(args, "scene_id", "/api/config/scene/config") }
fn save_scene(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_save(args, "scene_id", "/api/config/scene/config") }
fn delete_scene(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_delete(args, "scene_id", "/api/config/scene/config") }
fn get_script(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_get(args, "script_id", "/api/config/script/config") }
fn save_script(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_save(args, "script_id", "/api/config/script/config") }
fn delete_script(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult { crud_delete(args, "script_id", "/api/config/script/config") }
