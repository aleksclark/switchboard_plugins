mod tools;

pub mod arp {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/arp.v1.rs"));
    }
}

use arp::v1::*;
use prost::Message;
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
        version: "0.3.0".into(),
        abi_version: 1,
        description: "ARP agent registry and A2A agent management — manage projects, workspaces, spawn/stop/restart agents, send messages, discover agents, and route by skill via gRPC".into(),
        author: "aleksclark".into(),
        homepage: "https://github.com/aleksclark/switchboard_plugins".into(),
        license: "MIT".into(),
        capabilities: vec!["http".into()],
        credential_keys: vec!["base_url".into(), "token".into()],
        plain_text_keys: vec!["base_url".into()],
        optional_keys: vec!["token".into()],
        placeholders: HashMap::from([
            ("base_url".into(), "http://localhost:9098".into()),
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
    let req = ListProjectsRequest {};
    match grpc_call("arp.v1.ProjectService", "ListProjects", &req.encode_to_vec()) {
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
        "agents_proxy_list" => Some(proxy_list),
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
// gRPC transport over host_http_request with X-H2C
// ---------------------------------------------------------------------------

fn grpc_call(service: &str, method: &str, proto_body: &[u8]) -> Result<Vec<u8>, String> {
    let mut frame = Vec::with_capacity(5 + proto_body.len());
    frame.push(0u8); // not compressed
    frame.extend_from_slice(&(proto_body.len() as u32).to_be_bytes());
    frame.extend_from_slice(proto_body);

    let body_b64 = base64_encode(&frame);

    let mut headers = HashMap::new();
    headers.insert("Content-Type".into(), "application/grpc".into());
    headers.insert("TE".into(), "trailers".into());
    headers.insert("X-H2C".into(), "1".into());
    let tok = token();
    if !tok.is_empty() {
        headers.insert("Authorization".into(), format!("Bearer {}", tok));
    }

    let url = format!("{}/{}/{}", base_url(), service, method);
    let req = sdk::HttpRequest {
        method: "POST".into(),
        url,
        headers,
        body: body_b64,
    };

    let resp = sdk::host_http_request(&req)?;

    if resp.status != 200 {
        let grpc_status = resp.headers.get("grpc-status")
            .or_else(|| resp.headers.get("Grpc-Status"))
            .map(|s| s.as_str())
            .unwrap_or("?");
        let grpc_message = resp.headers.get("grpc-message")
            .or_else(|| resp.headers.get("Grpc-Message"))
            .map(|s| s.as_str())
            .unwrap_or(&resp.body);
        return Err(format!("gRPC error (http={}, grpc-status={}): {}", resp.status, grpc_status, grpc_message));
    }

    let resp_bytes = if resp.body.is_empty() {
        Vec::new()
    } else {
        base64_decode(&resp.body)?
    };

    if resp_bytes.len() < 5 {
        let grpc_status = resp.headers.get("grpc-status")
            .or_else(|| resp.headers.get("Grpc-Status"))
            .map(|s| s.as_str())
            .unwrap_or("0");
        if grpc_status != "0" {
            let grpc_message = resp.headers.get("grpc-message")
                .or_else(|| resp.headers.get("Grpc-Message"))
                .map(|s| s.as_str())
                .unwrap_or("unknown error");
            return Err(format!("gRPC error (status={}): {}", grpc_status, grpc_message));
        }
        return Ok(Vec::new());
    }

    let len = u32::from_be_bytes([resp_bytes[1], resp_bytes[2], resp_bytes[3], resp_bytes[4]]) as usize;
    if resp_bytes.len() < 5 + len {
        return Err("gRPC response truncated".into());
    }
    Ok(resp_bytes[5..5 + len].to_vec())
}

fn struct_to_json_value(s: &prost_types::Struct) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in &s.fields {
        map.insert(k.clone(), prost_value_to_json(v));
    }
    serde_json::Value::Object(map)
}

fn prost_value_to_json(v: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match &v.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => serde_json::json!(n),
        Some(Kind::StringValue(s)) => serde_json::json!(s),
        Some(Kind::BoolValue(b)) => serde_json::json!(b),
        Some(Kind::StructValue(s)) => struct_to_json_value(s),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(prost_value_to_json).collect())
        }
        None => serde_json::Value::Null,
    }
}

// Convert well-known prost-types messages to JSON
fn agent_instance_to_json(ai: &AgentInstance) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if !ai.id.is_empty() { m.insert("id".into(), serde_json::json!(ai.id)); }
    if !ai.template.is_empty() { m.insert("template".into(), serde_json::json!(ai.template)); }
    if !ai.workspace.is_empty() { m.insert("workspace".into(), serde_json::json!(ai.workspace)); }
    m.insert("status".into(), serde_json::json!(agent_status_str(ai.status)));
    if ai.port != 0 { m.insert("port".into(), serde_json::json!(ai.port)); }
    if !ai.direct_url.is_empty() { m.insert("direct_url".into(), serde_json::json!(ai.direct_url)); }
    if !ai.proxy_url.is_empty() { m.insert("proxy_url".into(), serde_json::json!(ai.proxy_url)); }
    if ai.pid != 0 { m.insert("pid".into(), serde_json::json!(ai.pid)); }
    if !ai.context_id.is_empty() { m.insert("context_id".into(), serde_json::json!(ai.context_id)); }
    if let Some(ref card) = ai.a2a_agent_card {
        m.insert("a2a_agent_card".into(), struct_to_json_value(card));
    }
    if !ai.token_id.is_empty() { m.insert("token_id".into(), serde_json::json!(ai.token_id)); }
    if !ai.session_id.is_empty() { m.insert("session_id".into(), serde_json::json!(ai.session_id)); }
    if !ai.spawned_by.is_empty() { m.insert("spawned_by".into(), serde_json::json!(ai.spawned_by)); }
    if let Some(ref ts) = ai.started_at {
        m.insert("started_at".into(), serde_json::json!(format!("{}s", ts.seconds)));
    }
    if let Some(ref md) = ai.metadata {
        m.insert("metadata".into(), struct_to_json_value(md));
    }
    serde_json::Value::Object(m)
}

fn agent_status_str(s: i32) -> &'static str {
    match s {
        1 => "AGENT_STATUS_STARTING",
        2 => "AGENT_STATUS_READY",
        3 => "AGENT_STATUS_BUSY",
        4 => "AGENT_STATUS_ERROR",
        5 => "AGENT_STATUS_STOPPING",
        6 => "AGENT_STATUS_STOPPED",
        _ => "AGENT_STATUS_UNSPECIFIED",
    }
}

fn workspace_to_json(ws: &Workspace) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if !ws.name.is_empty() { m.insert("name".into(), serde_json::json!(ws.name)); }
    if !ws.project.is_empty() { m.insert("project".into(), serde_json::json!(ws.project)); }
    if !ws.dir.is_empty() { m.insert("dir".into(), serde_json::json!(ws.dir)); }
    m.insert("status".into(), serde_json::json!(workspace_status_str(ws.status)));
    let agents: Vec<serde_json::Value> = ws.agents.iter().map(agent_instance_to_json).collect();
    m.insert("agents".into(), serde_json::Value::Array(agents));
    if let Some(ref ts) = ws.created_at {
        m.insert("created_at".into(), serde_json::json!(format!("{}s", ts.seconds)));
    }
    if let Some(ref md) = ws.metadata {
        m.insert("metadata".into(), struct_to_json_value(md));
    }
    serde_json::Value::Object(m)
}

fn workspace_status_str(s: i32) -> &'static str {
    match s {
        1 => "WORKSPACE_STATUS_ACTIVE",
        2 => "WORKSPACE_STATUS_INACTIVE",
        _ => "WORKSPACE_STATUS_UNSPECIFIED",
    }
}

fn project_to_json(p: &Project) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if !p.name.is_empty() { m.insert("name".into(), serde_json::json!(p.name)); }
    if !p.repo.is_empty() { m.insert("repo".into(), serde_json::json!(p.repo)); }
    if !p.branch.is_empty() { m.insert("branch".into(), serde_json::json!(p.branch)); }
    let agents: Vec<serde_json::Value> = p.agents.iter().map(|t| serde_json::json!({"name": t.name, "command": t.command})).collect();
    if !agents.is_empty() { m.insert("agents".into(), serde_json::Value::Array(agents)); }
    serde_json::Value::Object(m)
}

fn struct_resp_to_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let s = prost_types::Struct::decode(bytes).map_err(|e| format!("decode error: {e}"))?;
    Ok(struct_to_json_value(&s))
}

// ---------------------------------------------------------------------------
// Base64 (no_std-compatible, no external dep)
// ---------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 { out.push(B64[((triple >> 6) & 0x3F) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(B64[(triple & 0x3F) as usize] as char); } else { out.push('='); }
    }
    out
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in s.bytes() {
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'\n' | b'\r' | b' ' => continue,
            _ => return Err(format!("invalid base64 char: {}", c as char)),
        };
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Arg helpers
// ---------------------------------------------------------------------------

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
// A2A proxy helpers (still HTTP/JSON for /a2a/ endpoints)
// ---------------------------------------------------------------------------

fn a2a_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    let tok = token();
    if !tok.is_empty() {
        h.insert("Authorization".into(), format!("Bearer {}", tok));
    }
    h.insert("Content-Type".into(), "application/json".into());
    h
}

fn a2a_get(path: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "GET".into(),
        url: format!("{}{}", base_url(), path),
        headers: a2a_headers(),
        body: String::new(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("A2A proxy error ({}): {}", resp.status, resp.body));
    }
    if resp.body.trim().is_empty() { return Ok(r#"{"status":"success"}"#.into()); }
    Ok(resp.body)
}

fn a2a_post(path: &str, body: &str) -> Result<String, String> {
    let req = sdk::HttpRequest {
        method: "POST".into(),
        url: format!("{}{}", base_url(), path),
        headers: a2a_headers(),
        body: body.to_string(),
    };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 {
        return Err(format!("A2A proxy error ({}): {}", resp.status, resp.body));
    }
    if resp.body.trim().is_empty() { return Ok(r#"{"status":"success"}"#.into()); }
    Ok(resp.body)
}

// ---------------------------------------------------------------------------
// ProjectService RPCs — arp.v1.ProjectService
// ---------------------------------------------------------------------------

const PROJECT_SVC: &str = "arp.v1.ProjectService";

fn project_list(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let req = ListProjectsRequest {};
    match grpc_call(PROJECT_SVC, "ListProjects", &req.encode_to_vec()) {
        Ok(bytes) => {
            match ListProjectsResponse::decode(&bytes[..]) {
                Ok(resp) => {
                    let arr: Vec<serde_json::Value> = resp.projects.iter().map(project_to_json).collect();
                    sdk::raw_result(serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into()))
                }
                Err(e) => sdk::err_result(&format!("decode error: {e}")),
            }
        }
        Err(e) => sdk::err_result(&e),
    }
}

fn project_register(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let repo = match require_arg(&args, "repo") { Ok(v) => v, Err(r) => return r };

    let mut req = RegisterProjectRequest {
        name,
        repo,
        branch: sdk::arg_str(&args, "branch"),
        agents: Vec::new(),
    };

    if let Ok(Some(agents_json)) = parse_json_arg(&args, "agents") {
        if let Some(arr) = agents_json.as_array() {
            for a in arr {
                let mut tmpl = AgentTemplate::default();
                if let Some(n) = a.get("name").and_then(|v| v.as_str()) { tmpl.name = n.into(); }
                if let Some(c) = a.get("command").and_then(|v| v.as_str()) { tmpl.command = c.into(); }
                if let Some(p) = a.get("port_env").and_then(|v| v.as_str()) { tmpl.port_env = p.into(); }
                if let Some(caps) = a.get("capabilities").and_then(|v| v.as_array()) {
                    tmpl.capabilities = caps.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect();
                }
                if let Some(a2a) = a.get("a2a_card_config") {
                    let mut cfg = A2aCardConfig::default();
                    if let Some(n) = a2a.get("name").and_then(|v| v.as_str()) { cfg.name = n.into(); }
                    if let Some(d) = a2a.get("description").and_then(|v| v.as_str()) { cfg.description = d.into(); }
                    if let Some(skills) = a2a.get("skills").and_then(|v| v.as_array()) {
                        for sk in skills {
                            let mut skill = AgentSkillConfig::default();
                            if let Some(id) = sk.get("id").and_then(|v| v.as_str()) { skill.id = id.into(); }
                            if let Some(n) = sk.get("name").and_then(|v| v.as_str()) { skill.name = n.into(); }
                            if let Some(d) = sk.get("description").and_then(|v| v.as_str()) { skill.description = d.into(); }
                            if let Some(tags) = sk.get("tags").and_then(|v| v.as_array()) {
                                skill.tags = tags.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect();
                            }
                            cfg.skills.push(skill);
                        }
                    }
                    tmpl.a2a_card_config = Some(cfg);
                }
                req.agents.push(tmpl);
            }
        }
    } else if let Err(r) = parse_json_arg(&args, "agents") {
        return r;
    }

    match grpc_call(PROJECT_SVC, "RegisterProject", &req.encode_to_vec()) {
        Ok(bytes) => match Project::decode(&bytes[..]) {
            Ok(p) => sdk::raw_result(serde_json::to_string(&project_to_json(&p)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn project_unregister(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let req = UnregisterProjectRequest { name };
    match grpc_call(PROJECT_SVC, "UnregisterProject", &req.encode_to_vec()) {
        Ok(_) => sdk::raw_result(r#"{"status":"success"}"#.into()),
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// WorkspaceService RPCs — arp.v1.WorkspaceService
// ---------------------------------------------------------------------------

const WORKSPACE_SVC: &str = "arp.v1.WorkspaceService";

fn workspace_create(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let project = match require_arg(&args, "project") { Ok(v) => v, Err(r) => return r };

    let mut req = CreateWorkspaceRequest {
        name,
        project,
        branch: sdk::arg_str(&args, "branch"),
        auto_agents: Vec::new(),
    };

    if let Ok(Some(aa)) = parse_json_arg(&args, "auto_agents") {
        if let Some(arr) = aa.as_array() {
            req.auto_agents = arr.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect();
        }
    } else if let Err(r) = parse_json_arg(&args, "auto_agents") {
        return r;
    }

    match grpc_call(WORKSPACE_SVC, "CreateWorkspace", &req.encode_to_vec()) {
        Ok(bytes) => match Workspace::decode(&bytes[..]) {
            Ok(ws) => sdk::raw_result(serde_json::to_string(&workspace_to_json(&ws)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_list(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let status_str = sdk::arg_str(&args, "status");
    let status = match status_str.as_str() {
        "WORKSPACE_STATUS_ACTIVE" | "active" => WorkspaceStatus::Active as i32,
        "WORKSPACE_STATUS_INACTIVE" | "inactive" => WorkspaceStatus::Inactive as i32,
        _ => 0,
    };
    let req = ListWorkspacesRequest {
        project: sdk::arg_str(&args, "project"),
        status,
    };
    match grpc_call(WORKSPACE_SVC, "ListWorkspaces", &req.encode_to_vec()) {
        Ok(bytes) => match ListWorkspacesResponse::decode(&bytes[..]) {
            Ok(resp) => {
                let arr: Vec<serde_json::Value> = resp.workspaces.iter().map(workspace_to_json).collect();
                sdk::raw_result(serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into()))
            }
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_get(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let req = GetWorkspaceRequest { name };
    match grpc_call(WORKSPACE_SVC, "GetWorkspace", &req.encode_to_vec()) {
        Ok(bytes) => match Workspace::decode(&bytes[..]) {
            Ok(ws) => sdk::raw_result(serde_json::to_string(&workspace_to_json(&ws)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_destroy(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let req = DestroyWorkspaceRequest {
        name,
        keep_worktree: sdk::arg_bool(&args, "keep_worktree").unwrap_or(false),
    };
    match grpc_call(WORKSPACE_SVC, "DestroyWorkspace", &req.encode_to_vec()) {
        Ok(_) => sdk::raw_result(r#"{"status":"success"}"#.into()),
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// AgentService RPCs — arp.v1.AgentService
// ---------------------------------------------------------------------------

const AGENT_SVC: &str = "arp.v1.AgentService";

fn agent_spawn(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let workspace = match require_arg(&args, "workspace") { Ok(v) => v, Err(r) => return r };
    let template = match require_arg(&args, "template") { Ok(v) => v, Err(r) => return r };

    let mut req = SpawnAgentRequest {
        workspace,
        template,
        name: sdk::arg_str(&args, "name"),
        env: HashMap::new(),
        prompt: sdk::arg_str(&args, "prompt"),
        scope: None,
        permission: 0,
    };

    if let Ok(Some(env_json)) = parse_json_arg(&args, "env") {
        if let Some(obj) = env_json.as_object() {
            for (k, v) in obj {
                if let Some(s) = v.as_str() { req.env.insert(k.clone(), s.into()); }
            }
        }
    } else if let Err(r) = parse_json_arg(&args, "env") {
        return r;
    }

    if let Ok(Some(scope_json)) = parse_json_arg(&args, "scope") {
        let mut scope = Scope::default();
        if let Some(g) = scope_json.get("global").and_then(|v| v.as_bool()) { scope.global = g; }
        if let Some(projects) = scope_json.get("projects").and_then(|v| v.as_array()) {
            scope.projects = projects.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect();
        }
        req.scope = Some(scope);
    } else if let Err(r) = parse_json_arg(&args, "scope") {
        return r;
    }

    let perm_str = sdk::arg_str(&args, "permission");
    req.permission = match perm_str.as_str() {
        "PERMISSION_SESSION" | "session" => Permission::Session as i32,
        "PERMISSION_PROJECT" | "project" => Permission::Project as i32,
        "PERMISSION_ADMIN" | "admin" => Permission::Admin as i32,
        _ => 0,
    };

    match grpc_call(AGENT_SVC, "SpawnAgent", &req.encode_to_vec()) {
        Ok(bytes) => match AgentInstance::decode(&bytes[..]) {
            Ok(ai) => sdk::raw_result(serde_json::to_string(&agent_instance_to_json(&ai)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_list(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let status_str = sdk::arg_str(&args, "status");
    let status = match status_str.as_str() {
        "AGENT_STATUS_STARTING" | "starting" => AgentStatus::Starting as i32,
        "AGENT_STATUS_READY" | "ready" => AgentStatus::Ready as i32,
        "AGENT_STATUS_BUSY" | "busy" => AgentStatus::Busy as i32,
        "AGENT_STATUS_ERROR" | "error" => AgentStatus::Error as i32,
        "AGENT_STATUS_STOPPING" | "stopping" => AgentStatus::Stopping as i32,
        "AGENT_STATUS_STOPPED" | "stopped" => AgentStatus::Stopped as i32,
        _ => 0,
    };
    let req = ListAgentsRequest {
        workspace: sdk::arg_str(&args, "workspace"),
        status,
        template: sdk::arg_str(&args, "template"),
    };
    match grpc_call(AGENT_SVC, "ListAgents", &req.encode_to_vec()) {
        Ok(bytes) => match ListAgentsResponse::decode(&bytes[..]) {
            Ok(resp) => {
                let arr: Vec<serde_json::Value> = resp.agents.iter().map(agent_instance_to_json).collect();
                sdk::raw_result(serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into()))
            }
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_status(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let req = GetAgentStatusRequest { agent_id };
    match grpc_call(AGENT_SVC, "GetAgentStatus", &req.encode_to_vec()) {
        Ok(bytes) => match AgentInstance::decode(&bytes[..]) {
            Ok(ai) => sdk::raw_result(serde_json::to_string(&agent_instance_to_json(&ai)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_stop(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let grace = sdk::arg_str(&args, "grace_period_ms");
    let grace_ms = grace.parse::<i32>().unwrap_or(0);
    let req = StopAgentRequest { agent_id, grace_period_ms: grace_ms };
    match grpc_call(AGENT_SVC, "StopAgent", &req.encode_to_vec()) {
        Ok(bytes) => match AgentInstance::decode(&bytes[..]) {
            Ok(ai) => sdk::raw_result(serde_json::to_string(&agent_instance_to_json(&ai)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_restart(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let req = RestartAgentRequest { agent_id };
    match grpc_call(AGENT_SVC, "RestartAgent", &req.encode_to_vec()) {
        Ok(bytes) => match AgentInstance::decode(&bytes[..]) {
            Ok(ai) => sdk::raw_result(serde_json::to_string(&agent_instance_to_json(&ai)).unwrap()),
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let req = SendAgentMessageRequest {
        agent_id,
        message,
        context_id: sdk::arg_str(&args, "context_id"),
        blocking: sdk::arg_bool(&args, "blocking").unwrap_or(true),
    };
    match grpc_call(AGENT_SVC, "SendAgentMessage", &req.encode_to_vec()) {
        Ok(bytes) => match SendAgentMessageResponse::decode(&bytes[..]) {
            Ok(resp) => {
                let val = match resp.result {
                    Some(send_agent_message_response::Result::Task(s)) => {
                        serde_json::json!({"task": struct_to_json_value(&s)})
                    }
                    Some(send_agent_message_response::Result::Message(s)) => {
                        serde_json::json!({"message": struct_to_json_value(&s)})
                    }
                    None => serde_json::json!({"status": "sent"}),
                };
                sdk::raw_result(serde_json::to_string(&val).unwrap())
            }
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let req = CreateAgentTaskRequest {
        agent_id,
        message,
        context_id: sdk::arg_str(&args, "context_id"),
    };
    match grpc_call(AGENT_SVC, "CreateAgentTask", &req.encode_to_vec()) {
        Ok(bytes) => match struct_resp_to_json(&bytes) {
            Ok(val) => sdk::raw_result(serde_json::to_string(&val).unwrap()),
            Err(e) => sdk::err_result(&e),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_task_status(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    let hl = sdk::arg_str(&args, "history_length").parse::<i32>().unwrap_or(0);
    let req = GetAgentTaskStatusRequest {
        agent_id,
        task_id,
        history_length: hl,
    };
    match grpc_call(AGENT_SVC, "GetAgentTaskStatus", &req.encode_to_vec()) {
        Ok(bytes) => match struct_resp_to_json(&bytes) {
            Ok(val) => sdk::raw_result(serde_json::to_string(&val).unwrap()),
            Err(e) => sdk::err_result(&e),
        },
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// DiscoveryService RPCs — arp.v1.DiscoveryService
// ---------------------------------------------------------------------------

const DISCOVERY_SVC: &str = "arp.v1.DiscoveryService";

fn discover(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let scope_str = sdk::arg_str(&args, "scope");
    let scope = match scope_str.as_str() {
        "DISCOVERY_SCOPE_LOCAL" | "local" => DiscoveryScope::Local as i32,
        "DISCOVERY_SCOPE_NETWORK" | "network" => DiscoveryScope::Network as i32,
        _ => 0,
    };

    let mut urls = Vec::new();
    if let Ok(Some(urls_json)) = parse_json_arg(&args, "urls") {
        if let Some(arr) = urls_json.as_array() {
            urls = arr.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect();
        }
    } else if let Err(r) = parse_json_arg(&args, "urls") {
        return r;
    }

    let req = DiscoverAgentsRequest {
        scope,
        capability: sdk::arg_str(&args, "capability"),
        urls,
    };
    match grpc_call(DISCOVERY_SVC, "DiscoverAgents", &req.encode_to_vec()) {
        Ok(bytes) => match DiscoverAgentsResponse::decode(&bytes[..]) {
            Ok(resp) => {
                let arr: Vec<serde_json::Value> = resp.agent_cards.iter().map(struct_to_json_value).collect();
                sdk::raw_result(serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into()))
            }
            Err(e) => sdk::err_result(&format!("decode error: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// A2A proxy endpoints (HTTP/JSON — served by ARP HTTP server)
// These remain HTTP because the A2A proxy is an HTTP layer, not gRPC.
// The base_url for these is the HTTP server (typically port 9099).
// When base_url points to the gRPC port (9098), these will fail —
// users should configure a separate a2a_base_url or use agents_discover.
// ---------------------------------------------------------------------------

fn proxy_list(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match a2a_get("/a2a/agents") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_card(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match a2a_get(&format!("/a2a/agents/{}/.well-known/agent-card.json", encode_path(&agent_id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn proxy_send_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message_text = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let message_id = { let v = sdk::arg_str(&args, "message_id"); if v.is_empty() { gen_message_id() } else { v } };

    let mut msg = serde_json::json!({
        "role": "ROLE_USER",
        "parts": [{"text": message_text}],
        "message_id": message_id,
    });
    let context_id = sdk::arg_str(&args, "context_id");
    if !context_id.is_empty() { msg["context_id"] = serde_json::Value::String(context_id); }

    let body = serde_json::json!({"message": msg});
    match a2a_post(&format!("/a2a/agents/{}/message:send", encode_path(&agent_id)), &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn proxy_get_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    match a2a_get(&format!("/a2a/agents/{}/tasks/{}", encode_path(&agent_id), encode_path(&task_id))) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn proxy_cancel_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    match a2a_post(&format!("/a2a/agents/{}/tasks/{}:cancel", encode_path(&agent_id), encode_path(&task_id)), "{}") {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}

fn route_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let message_text = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let tags = match parse_json_arg(&args, "tags") {
        Ok(Some(v)) => v,
        Ok(None) => return sdk::err_result("tags is required"),
        Err(r) => return r,
    };
    let message_id = { let v = sdk::arg_str(&args, "message_id"); if v.is_empty() { gen_message_id() } else { v } };

    let mut msg = serde_json::json!({
        "role": "ROLE_USER",
        "parts": [{"text": message_text}],
        "message_id": message_id,
    });
    let context_id = sdk::arg_str(&args, "context_id");
    if !context_id.is_empty() { msg["context_id"] = serde_json::Value::String(context_id); }

    let body = serde_json::json!({"message": msg, "routing": {"tags": tags}});
    match a2a_post("/a2a/route/message:send", &body.to_string()) {
        Ok(d) => sdk::raw_result(d),
        Err(e) => sdk::err_result(&e),
    }
}
