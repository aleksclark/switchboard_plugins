mod tools;

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use switchboard_guest_sdk as sdk;

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

#[derive(Clone)]
struct Config {
    base_url: String,
    token: String,
}

type Args = HashMap<String, Value>;

fn config() -> Result<Config, String> {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "platen: not configured".to_string())
}

#[no_mangle]
pub extern "C" fn name() -> u64 {
    sdk::leaked_string("platen")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&sdk::PluginMetadata {
        name: "platen".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        abi_version: 1,
        description: "Platen printer and scanner server: print documents, check ink and queues, scan pages, and file scans in Paperless-ngx".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec!["base_url".into(), "token".into()],
        plain_text_keys: vec!["base_url".into()],
        optional_keys: vec![],
        placeholders: HashMap::from([
            ("base_url".into(), "https://platen.example.lan".into()),
            ("token".into(), "A token from PLATEN_AUTH_TOKENS".into()),
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
    let creds: HashMap<String, String> = match serde_json::from_slice(&input) {
        Ok(c) => c,
        Err(e) => return sdk::leaked_string(&format!("invalid credentials JSON: {e}")),
    };
    match parse_config(&creds) {
        Ok(cfg) => {
            *CONFIG.lock().unwrap() = Some(cfg);
            0
        }
        Err(e) => sdk::leaked_string(&e),
    }
}

fn parse_config(creds: &HashMap<String, String>) -> Result<Config, String> {
    let base_url = creds
        .get("base_url")
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .unwrap_or_default();
    if base_url.is_empty() {
        return Err("platen: base_url is required".into());
    }
    if !base_url.starts_with("https://") && !base_url.starts_with("http://") {
        return Err("platen: base_url must be an http(s) URL".into());
    }
    let token = creds
        .get("token")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if token.is_empty() {
        return Err("platen: token is required".into());
    }
    Ok(Config { base_url, token })
}

#[no_mangle]
pub extern "C" fn execute(ptr_size: u64) -> u64 {
    let input = sdk::read_input(ptr_size);
    let result = match serde_json::from_slice::<sdk::ExecuteRequest>(&input) {
        Ok(req) => dispatch(&req.tool_name, &req.args),
        Err(e) => sdk::err_result(&format!("invalid request: {e}")),
    };
    sdk::leaked_result(&serde_json::to_vec(&result).unwrap_or_default())
}

#[no_mangle]
pub extern "C" fn healthy() -> i32 {
    match call("GET", "/readyz", None) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn dispatch(tool: &str, args: &Args) -> sdk::ToolResult {
    let outcome = match tool {
        "platen_list_devices" => list_devices(),
        "platen_printer_status" => required(args, "printer_id")
            .and_then(|id| get(&format!("/api/v1/printers/{}", enc(&id)))),
        "platen_printer_queue" => required(args, "printer_id")
            .and_then(|id| get(&format!("/api/v1/printers/{}/queue", enc(&id)))),
        "platen_print_document" => return print_document(args),
        "platen_list_print_jobs" => get(&format!("/api/v1/jobs?limit={}", limit(args, 50))),
        "platen_cancel_print_job" => required(args, "job_id")
            .and_then(|id| call("DELETE", &format!("/api/v1/jobs/{}", enc(&id)), None)),
        "platen_scan_document" => post("/api/v1/scans", scan_body(args)),
        "platen_scan_next_page" => required(args, "scan_id")
            .and_then(|id| post(&format!("/api/v1/scans/{}/pages", enc(&id)), json!({}))),
        "platen_get_scan" => {
            required(args, "scan_id").and_then(|id| get(&format!("/api/v1/scans/{}", enc(&id))))
        }
        "platen_list_scans" => get(&format!("/api/v1/scans?limit={}", limit(args, 50))),
        "platen_rename_scan" => required(args, "scan_id").and_then(|id| {
            let title = required(args, "title")?;
            call(
                "PATCH",
                &format!("/api/v1/scans/{}", enc(&id)),
                Some(json!({ "title": title })),
            )
        }),
        "platen_delete_scan" => required(args, "scan_id")
            .and_then(|id| call("DELETE", &format!("/api/v1/scans/{}", enc(&id)), None)),
        "platen_file_scan_in_paperless" => required(args, "scan_id").and_then(|id| {
            post(
                &format!("/api/v1/scans/{}/paperless", enc(&id)),
                file_body(args),
            )
        }),
        "platen_search_paperless" => required(args, "query").and_then(|q| {
            get(&format!(
                "/api/v1/paperless/documents?query={}&limit={}",
                enc(&q),
                limit(args, 20)
            ))
        }),
        _ => Err(format!("unknown tool: {tool}")),
    };
    match outcome {
        Ok(body) => sdk::raw_result(body),
        Err(e) => sdk::err_result(&e),
    }
}

fn list_devices() -> Result<String, String> {
    let printers: Value =
        serde_json::from_str(&get("/api/v1/printers")?).map_err(|e| e.to_string())?;
    let scanners: Value =
        serde_json::from_str(&get("/api/v1/scanners")?).map_err(|e| e.to_string())?;
    Ok(json!({
        "printers": printers.get("printers").cloned().unwrap_or(Value::Null),
        "scanners": scanners.get("scanners").cloned().unwrap_or(Value::Null),
    })
    .to_string())
}

fn print_document(args: &Args) -> sdk::ToolResult {
    let body = match print_body(args) {
        Ok(b) => b,
        Err(e) => return sdk::err_result(&e),
    };
    let resp = match send("POST", "/api/v1/print", Some(body)) {
        Ok(r) => r,
        Err(e) => return sdk::err_result(&e),
    };
    if resp.status == 409 && resp.body.contains("confirmation_required") {
        // Not a failure: the person has to approve the paper use first.
        return sdk::raw_result(
            json!({
                "status": "confirmation_required",
                "instructions": "Nothing was printed. Ask the person whether to print this job; only if they agree, repeat the same call with confirm=true.",
                "platen": serde_json::from_str::<Value>(&resp.body).unwrap_or(Value::String(resp.body.clone())),
            })
            .to_string(),
        );
    }
    match check(resp) {
        Ok(b) => sdk::raw_result(b),
        Err(e) => sdk::err_result(&e),
    }
}

fn print_body(args: &Args) -> Result<Value, String> {
    let mut source = Map::new();
    let mut count = 0;
    for (arg, key) in [
        ("text", "text"),
        ("url", "url"),
        ("content_base64", "base64"),
        ("scan_id", "scan_id"),
    ] {
        let v = sdk::arg_str(args, arg);
        if !v.is_empty() {
            source.insert(key.into(), Value::String(v));
            count += 1;
        }
    }
    if let Some(id) = sdk::arg_int(args, "paperless_document_id") {
        if id <= 0 {
            return Err("paperless_document_id must be a positive integer".into());
        }
        source.insert("paperless_id".into(), json!(id));
        count += 1;
    }
    if count != 1 {
        return Err(
            "give exactly one source: text, url, content_base64, scan_id or paperless_document_id"
                .into(),
        );
    }
    if source.contains_key("base64") {
        for (arg, key) in [("filename", "name"), ("mime", "mime")] {
            let v = sdk::arg_str(args, arg);
            if !v.is_empty() {
                source.insert(key.into(), Value::String(v));
            }
        }
    }
    let mut body = Map::new();
    body.insert("source".into(), Value::Object(source));
    for key in [
        "printer", "title", "duplex", "color", "quality", "media", "pages",
    ] {
        let v = sdk::arg_str(args, key);
        if !v.is_empty() {
            body.insert(key.into(), Value::String(v));
        }
    }
    if let Some(copies) = sdk::arg_int(args, "copies") {
        if copies < 1 {
            return Err("copies must be at least 1".into());
        }
        body.insert("copies".into(), json!(copies));
    }
    for key in ["fill", "confirm", "dry_run"] {
        if sdk::arg_bool(args, key) == Some(true) {
            body.insert(key.into(), Value::Bool(true));
        }
    }
    Ok(Value::Object(body))
}

fn scan_body(args: &Args) -> Value {
    let mut body = Map::new();
    for key in ["scanner", "source", "color", "paper", "title"] {
        let v = sdk::arg_str(args, key);
        if !v.is_empty() {
            body.insert(key.into(), Value::String(v));
        }
    }
    if let Some(dpi) = sdk::arg_int(args, "resolution") {
        body.insert("resolution".into(), json!(dpi));
    }
    if sdk::arg_bool(args, "duplex") == Some(true) {
        body.insert("duplex".into(), Value::Bool(true));
    }
    Value::Object(body)
}

fn file_body(args: &Args) -> Value {
    let mut body = Map::new();
    for key in ["title", "correspondent", "document_type", "created"] {
        let v = sdk::arg_str(args, key);
        if !v.is_empty() {
            body.insert(key.into(), Value::String(v));
        }
    }
    let tags = sdk::arg_str_slice(args, "tags");
    if !tags.is_empty() {
        body.insert("tags".into(), json!(tags));
    }
    if sdk::arg_bool(args, "wait") == Some(true) {
        body.insert("wait".into(), Value::Bool(true));
    }
    Value::Object(body)
}

fn required(args: &Args, key: &str) -> Result<String, String> {
    let v = sdk::arg_str(args, key);
    if v.is_empty() {
        Err(format!("{key} is required"))
    } else {
        Ok(v)
    }
}

fn limit(args: &Args, default: i64) -> i64 {
    sdk::arg_int(args, "limit")
        .filter(|n| *n > 0)
        .unwrap_or(default)
        .min(500)
}

fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn get(path: &str) -> Result<String, String> {
    call("GET", path, None)
}

fn post(path: &str, body: Value) -> Result<String, String> {
    call("POST", path, Some(body))
}

fn call(method: &str, path: &str, body: Option<Value>) -> Result<String, String> {
    check(send(method, path, body)?)
}

fn send(method: &str, path: &str, body: Option<Value>) -> Result<sdk::HttpResponse, String> {
    let cfg = config()?;
    let mut headers = HashMap::from([
        ("Authorization".to_string(), format!("Bearer {}", cfg.token)),
        ("Accept".to_string(), "application/json".to_string()),
    ]);
    if body.is_some() {
        headers.insert("Content-Type".into(), "application/json".into());
    }
    sdk::host_http_request(&sdk::HttpRequest {
        method: method.into(),
        url: format!("{}{}", cfg.base_url, path),
        headers,
        body: body.map(|b| b.to_string()).unwrap_or_default(),
        body_base64: String::new(),
    })
}

fn check(resp: sdk::HttpResponse) -> Result<String, String> {
    if resp.status >= 400 {
        return Err(format!("platen API error ({}): {}", resp.status, resp.body));
    }
    if resp.body.trim().is_empty() {
        return Ok(r#"{"status":"ok"}"#.into());
    }
    Ok(resp.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: Value) -> Args {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn print_body_requires_exactly_one_source() {
        assert!(print_body(&args(json!({}))).is_err());
        assert!(print_body(&args(json!({"text": "a", "url": "https://x"}))).is_err());
        let b = print_body(&args(
            json!({"text": "hi", "printer": "house", "copies": "2", "dry_run": true}),
        ))
        .unwrap();
        assert_eq!(
            b,
            json!({"source": {"text": "hi"}, "printer": "house", "copies": 2, "dry_run": true})
        );
    }

    #[test]
    fn print_body_maps_base64_and_paperless() {
        let b = print_body(&args(
            json!({"content_base64": "AA==", "filename": "a.pdf", "mime": "application/pdf"}),
        ))
        .unwrap();
        assert_eq!(
            b["source"],
            json!({"base64": "AA==", "name": "a.pdf", "mime": "application/pdf"})
        );
        let b = print_body(&args(
            json!({"paperless_document_id": 12, "filename": "ignored"}),
        ))
        .unwrap();
        assert_eq!(b["source"], json!({"paperless_id": 12}));
        assert!(print_body(&args(json!({"paperless_document_id": 0}))).is_err());
        assert!(print_body(&args(json!({"text": "x", "copies": 0}))).is_err());
    }

    #[test]
    fn confirm_only_when_true() {
        let b = print_body(&args(
            json!({"text": "x", "confirm": false, "fill": "true"}),
        ))
        .unwrap();
        assert!(b.get("confirm").is_none());
        assert_eq!(b["fill"], json!(true));
    }

    #[test]
    fn config_validation() {
        let ok = HashMap::from([
            ("base_url".to_string(), "https://p.lan/".to_string()),
            ("token".to_string(), "t".to_string()),
        ]);
        assert_eq!(parse_config(&ok).unwrap().base_url, "https://p.lan");
        let bad = HashMap::from([
            ("base_url".to_string(), "p.lan".to_string()),
            ("token".to_string(), "t".to_string()),
        ]);
        assert!(parse_config(&bad).is_err());
        let no_tok = HashMap::from([("base_url".to_string(), "https://p.lan".to_string())]);
        assert!(parse_config(&no_tok).is_err());
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
        assert_eq!(limit(&args(json!({"limit": 9999})), 50), 500);
        assert_eq!(limit(&args(json!({})), 50), 50);
    }

    #[test]
    fn every_tool_is_dispatched_once() {
        let src = include_str!("lib.rs");
        let defs = tools::tool_definitions();
        let mut names: Vec<_> = defs.iter().map(|t| t.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), defs.len());
        for name in names {
            assert!(name.starts_with("platen_"));
            assert_eq!(src.matches(&format!("\"{name}\" =>")).count(), 1, "{name}");
        }
    }
}
