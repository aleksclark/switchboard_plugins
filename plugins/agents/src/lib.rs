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
    sdk::leaked_string("agents")
}

#[no_mangle]
pub extern "C" fn metadata() -> u64 {
    sdk::leaked_metadata(&sdk::PluginMetadata {
        name: "agents".into(),
        version: "0.1.0".into(),
        abi_version: 1,
        description: "ARP agent registry and A2A agent interaction — manage projects, workspaces, spawn/stop agents, send messages, discover agents, and route by skill".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec!["base_url".into(), "token".into()],
        plain_text_keys: vec!["base_url".into()],
        optional_keys: vec!["token".into()],
        placeholders: HashMap::from([
            ("base_url".into(), "http://localhost:9099".into()),
            ("token".into(), "arp-bearer-token (optional for localhost)".into()),
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

    let bu = creds
        .get("base_url")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_default();
    if bu.is_empty() {
        return sdk::leaked_string("agents: base_url is required");
    }

    let tok = creds.get("token").cloned().unwrap_or_default();

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
    match do_get("/api/projects") {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

type HandlerFn = fn(HashMap<String, serde_json::Value>) -> sdk::ToolResult;

fn dispatch(tool_name: &str, args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let handler: Option<HandlerFn> = match tool_name {
        "agents_project_list" => Some(project_list),
        "agents_project_register" => Some(project_register),
        "agents_project_unregister" => Some(project_unregister),
        "agents_workspace_create" => Some(workspace_create),
        "agents_workspace_list" => Some(workspace_list),
        "agents_workspace_get" => Some(workspace_get),
        "agents_workspace_destroy" => Some(workspace_destroy),
        "agents_agent_spawn" => Some(agent_spawn),
        "agents_agent_list" => Some(agent_list),
        "agents_agent_status" => Some(agent_status),
        "agents_agent_stop" => Some(agent_stop),
        "agents_agent_restart" => Some(agent_restart),
        "agents_agent_message" => Some(agent_message),
        "agents_agent_task" => Some(agent_task),
        "agents_agent_task_status" => Some(agent_task_status),
        "agents_discover" => Some(discover),
        "agents_agent_card" => Some(agent_card),
        "agents_proxy_send_message" => Some(proxy_send_message),
        "agents_proxy_get_task" => Some(proxy_get_task),
        "agents_proxy_cancel_task" => Some(proxy_cancel_task),
        "agents_route_message" => Some(route_message),
        _ => None,
    };

    match handler {
        Some(f) => f(args),
        None => sdk::err_result(&format!("unknown tool: {tool_name}")),
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn auth_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    let tok = token();
    if !tok.is_empty() {
        h.insert("Authorization".into(), format!("Bearer {}", tok));
    }
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
        return Err(format!("ARP API error ({}): {}", resp.status, resp.body));
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
        return Err(format!("ARP API error ({}): {}", resp.status, resp.body));
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

fn parse_json_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Result<Option<serde_json::Value>, sdk::ToolResult> {
    let v = sdk::arg_str(args, key);
    if v.is_empty() {
        return Ok(None);
    }
    let parsed: serde_json::Value = serde_json::from_str(&v)
        .map_err(|e| sdk::err_result(&format!("invalid JSON for {key}: {e}")))?;
    Ok(Some(parsed))
}

fn encode_path(s: &str) -> String {
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

fn gen_message_id() -> String {
    format!("swb-{:016x}", {
        static COUNTER: Mutex<u64> = Mutex::new(0);
        let mut c = COUNTER.lock().unwrap();
        *c += 1;
        *c
    })
}

// ---------------------------------------------------------------------------
// MCP tool call helpers — ARP exposes its management via an HTTP API that
// mirrors the MCP tools. We call the REST endpoints under /api/.
// ---------------------------------------------------------------------------

// ===== Project management =====

fn project_list(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/api/projects") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn project_register(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let repo = match require_arg(&args, "repo") { Ok(v) => v, Err(r) => return r };

    let mut body = serde_json::json!({
        "name": name,
        "repo": repo,
    });

    if let Ok(Some(agents)) = parse_json_arg(&args, "agents") {
        body["agents"] = agents;
    } else if let Err(r) = parse_json_arg(&args, "agents") {
        return r;
    }

    match do_post("/api/projects", &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn project_unregister(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let req = sdk::HttpRequest {
        method: "DELETE".into(),
        url: format!("{}/api/projects/{}", base_url(), encode_path(&name)),
        headers: auth_headers(),
        body: String::new(),
    };
    match sdk::host_http_request(&req) {
        Ok(resp) => {
            if resp.status >= 400 {
                sdk::err_result(&format!("ARP API error ({}): {}", resp.status, resp.body))
            } else if resp.body.is_empty() {
                sdk::raw_result(r#"{"status":"success"}"#.into())
            } else {
                sdk::raw_result(resp.body)
            }
        }
        Err(e) => sdk::err_result(&e),
    }
}

// ===== Workspace management =====

fn workspace_create(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let project = match require_arg(&args, "project") { Ok(v) => v, Err(r) => return r };

    let mut body = serde_json::json!({
        "name": name,
        "project": project,
    });

    if let Ok(Some(auto_agents)) = parse_json_arg(&args, "auto_agents") {
        body["auto_agents"] = auto_agents;
    } else if let Err(r) = parse_json_arg(&args, "auto_agents") {
        return r;
    }

    match do_post("/api/workspaces", &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_list(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let mut params = Vec::new();
    let project = sdk::arg_str(&args, "project");
    if !project.is_empty() {
        params.push(format!("project={}", encode_path(&project)));
    }
    let status = sdk::arg_str(&args, "status");
    if !status.is_empty() {
        params.push(format!("status={}", encode_path(&status)));
    }
    let mut path = "/api/workspaces".to_string();
    if !params.is_empty() {
        path.push('?');
        path.push_str(&params.join("&"));
    }
    match do_get(&path) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_get(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/workspaces/{}", encode_path(&name))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_destroy(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let keep_worktree = sdk::arg_bool(&args, "keep_worktree").unwrap_or(false);

    let mut path = format!("/api/workspaces/{}", encode_path(&name));
    if keep_worktree {
        path.push_str("?keep_worktree=true");
    }

    let req = sdk::HttpRequest {
        method: "DELETE".into(),
        url: format!("{}{}", base_url(), path),
        headers: auth_headers(),
        body: String::new(),
    };
    match sdk::host_http_request(&req) {
        Ok(resp) => {
            if resp.status >= 400 {
                sdk::err_result(&format!("ARP API error ({}): {}", resp.status, resp.body))
            } else if resp.body.is_empty() {
                sdk::raw_result(r#"{"status":"success"}"#.into())
            } else {
                sdk::raw_result(resp.body)
            }
        }
        Err(e) => sdk::err_result(&e),
    }
}

// ===== Agent lifecycle =====

fn agent_spawn(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let workspace = match require_arg(&args, "workspace") { Ok(v) => v, Err(r) => return r };
    let template = match require_arg(&args, "template") { Ok(v) => v, Err(r) => return r };

    let mut body = serde_json::json!({
        "workspace": workspace,
        "template": template,
    });

    let name = sdk::arg_str(&args, "name");
    if !name.is_empty() {
        body["name"] = serde_json::Value::String(name);
    }

    let prompt = sdk::arg_str(&args, "prompt");
    if !prompt.is_empty() {
        body["prompt"] = serde_json::Value::String(prompt);
    }

    if let Ok(Some(scope)) = parse_json_arg(&args, "scope") {
        body["scope"] = scope;
    } else if let Err(r) = parse_json_arg(&args, "scope") {
        return r;
    }

    let permission = sdk::arg_str(&args, "permission");
    if !permission.is_empty() {
        body["permission"] = serde_json::Value::String(permission);
    }

    match do_post("/api/agents", &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_list(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let mut params = Vec::new();
    let workspace = sdk::arg_str(&args, "workspace");
    if !workspace.is_empty() {
        params.push(format!("workspace={}", encode_path(&workspace)));
    }
    let status = sdk::arg_str(&args, "status");
    if !status.is_empty() {
        params.push(format!("status={}", encode_path(&status)));
    }
    let template = sdk::arg_str(&args, "template");
    if !template.is_empty() {
        params.push(format!("template={}", encode_path(&template)));
    }
    let mut path = "/api/agents".to_string();
    if !params.is_empty() {
        path.push('?');
        path.push_str(&params.join("&"));
    }
    match do_get(&path) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_status(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/api/agents/{}", encode_path(&agent_id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_stop(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };

    let mut body = serde_json::json!({});

    let grace = sdk::arg_str(&args, "grace_period_ms");
    if !grace.is_empty() {
        if let Ok(ms) = grace.parse::<u64>() {
            body["grace_period_ms"] = serde_json::Value::Number(ms.into());
        }
    }

    match do_post(&format!("/api/agents/{}/stop", encode_path(&agent_id)), &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_restart(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match do_post(&format!("/api/agents/{}/restart", encode_path(&agent_id)), "{}") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ===== Agent messaging =====

fn agent_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };

    let mut body = serde_json::json!({
        "message": message,
    });

    let context_id = sdk::arg_str(&args, "context_id");
    if !context_id.is_empty() {
        body["context_id"] = serde_json::Value::String(context_id);
    }

    let blocking = sdk::arg_bool(&args, "blocking");
    if let Some(b) = blocking {
        body["blocking"] = serde_json::Value::Bool(b);
    }

    match do_post(&format!("/api/agents/{}/message", encode_path(&agent_id)), &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };

    let mut body = serde_json::json!({
        "message": message,
    });

    let context_id = sdk::arg_str(&args, "context_id");
    if !context_id.is_empty() {
        body["context_id"] = serde_json::Value::String(context_id);
    }

    match do_post(&format!("/api/agents/{}/task", encode_path(&agent_id)), &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_task_status(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };

    let mut path = format!("/api/agents/{}/tasks/{}", encode_path(&agent_id), encode_path(&task_id));

    let history_length = sdk::arg_str(&args, "history_length");
    if !history_length.is_empty() {
        path.push_str(&format!("?history_length={}", encode_path(&history_length)));
    }

    match do_get(&path) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ===== A2A proxy discovery =====

fn discover(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match do_get("/a2a/agents") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_card(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/a2a/agents/{}/.well-known/agent-card.json", encode_path(&agent_id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ===== A2A proxy messaging =====

fn proxy_send_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message_text = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };

    let message_id = {
        let v = sdk::arg_str(&args, "message_id");
        if v.is_empty() { gen_message_id() } else { v }
    };

    let mut msg = serde_json::json!({
        "role": "ROLE_USER",
        "parts": [{"text_part": {"text": message_text}}],
    });

    let context_id = sdk::arg_str(&args, "context_id");
    if !context_id.is_empty() {
        msg["context_id"] = serde_json::Value::String(context_id);
    }

    let body = serde_json::json!({
        "message": msg,
        "message_id": message_id,
    });

    match do_post(
        &format!("/a2a/agents/{}/message:send", encode_path(&agent_id)),
        &body.to_string(),
    ) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn proxy_get_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    match do_get(&format!("/a2a/agents/{}/tasks/{}", encode_path(&agent_id), encode_path(&task_id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn proxy_cancel_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    match do_post(
        &format!("/a2a/agents/{}/tasks/{}:cancel", encode_path(&agent_id), encode_path(&task_id)),
        "{}",
    ) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

// ===== A2A proxy routing =====

fn route_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let message_text = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let tags = match parse_json_arg(&args, "tags") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("tags is required"),
        Err(r) => return r,
    };

    let message_id = {
        let v = sdk::arg_str(&args, "message_id");
        if v.is_empty() { gen_message_id() } else { v }
    };

    let mut msg = serde_json::json!({
        "role": "ROLE_USER",
        "parts": [{"text_part": {"text": message_text}}],
    });

    let context_id = sdk::arg_str(&args, "context_id");
    if !context_id.is_empty() {
        msg["context_id"] = serde_json::Value::String(context_id);
    }

    let body = serde_json::json!({
        "message": msg,
        "message_id": message_id,
        "routing": {
            "tags": tags,
        },
    });

    match do_post("/a2a/route/message:send", &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}
