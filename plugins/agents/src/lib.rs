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
        version: "0.4.0".into(),
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
    *CONFIG.lock().unwrap() = Some(Config { base_url: bu, token: tok });
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
// gRPC transport — binary protobuf over h2c via SDK body_base64
// ---------------------------------------------------------------------------

fn grpc_call(service: &str, method: &str, proto_body: &[u8]) -> Result<Vec<u8>, String> {
    let mut frame = Vec::with_capacity(5 + proto_body.len());
    frame.push(0u8);
    frame.extend_from_slice(&(proto_body.len() as u32).to_be_bytes());
    frame.extend_from_slice(proto_body);

    let mut headers = HashMap::new();
    headers.insert("Content-Type".into(), "application/grpc".into());
    headers.insert("TE".into(), "trailers".into());
    headers.insert("X-H2C".into(), "1".into());
    let tok = token();
    if !tok.is_empty() {
        headers.insert("Authorization".into(), format!("Bearer {}", tok));
    }

    let url = format!("{}/{}/{}", base_url(), service, method);
    let req = sdk::HttpRequest::with_body_bytes("POST", &url, headers, &frame);
    let resp = sdk::host_http_request(&req)?;

    if resp.status != 200 {
        let gs = grpc_status_header(&resp, "grpc-status").unwrap_or("?".into());
        let gm = grpc_status_header(&resp, "grpc-message").unwrap_or(resp.body.clone());
        return Err(format!("gRPC error (http={}, status={}): {}", resp.status, gs, gm));
    }

    let resp_bytes = resp.body_bytes()?;

    if resp_bytes.len() < 5 {
        let gs = grpc_status_header(&resp, "grpc-status").unwrap_or("0".into());
        if gs != "0" {
            let gm = grpc_status_header(&resp, "grpc-message").unwrap_or("unknown".into());
            return Err(format!("gRPC error (status={}): {}", gs, gm));
        }
        return Ok(Vec::new());
    }

    let len = u32::from_be_bytes([resp_bytes[1], resp_bytes[2], resp_bytes[3], resp_bytes[4]]) as usize;
    if resp_bytes.len() < 5 + len {
        return Err("gRPC response truncated".into());
    }
    Ok(resp_bytes[5..5 + len].to_vec())
}

fn grpc_status_header(resp: &sdk::HttpResponse, key: &str) -> Option<String> {
    resp.headers.get(key)
        .or_else(|| {
            let title = key.split('-').map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            }).collect::<Vec<_>>().join("-");
            resp.headers.get(&title)
        })
        .cloned()
}

// ---------------------------------------------------------------------------
// JSON conversion helpers for prost types
// ---------------------------------------------------------------------------

fn struct_to_json(s: &prost_types::Struct) -> serde_json::Value {
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
        Some(Kind::StructValue(s)) => struct_to_json(s),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(prost_value_to_json).collect())
        }
        None => serde_json::Value::Null,
    }
}

fn agent_instance_to_json(ai: &AgentInstance) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if !ai.id.is_empty() { m.insert("id".into(), serde_json::json!(ai.id)); }
    if !ai.template.is_empty() { m.insert("template".into(), serde_json::json!(ai.template)); }
    if !ai.workspace.is_empty() { m.insert("workspace".into(), serde_json::json!(ai.workspace)); }
    m.insert("status".into(), serde_json::json!(ai.status));
    if ai.port != 0 { m.insert("port".into(), serde_json::json!(ai.port)); }
    if !ai.direct_url.is_empty() { m.insert("direct_url".into(), serde_json::json!(ai.direct_url)); }
    if !ai.proxy_url.is_empty() { m.insert("proxy_url".into(), serde_json::json!(ai.proxy_url)); }
    if ai.pid != 0 { m.insert("pid".into(), serde_json::json!(ai.pid)); }
    if !ai.context_id.is_empty() { m.insert("context_id".into(), serde_json::json!(ai.context_id)); }
    if let Some(ref card) = ai.a2a_agent_card { m.insert("a2a_agent_card".into(), struct_to_json(card)); }
    if !ai.token_id.is_empty() { m.insert("token_id".into(), serde_json::json!(ai.token_id)); }
    if !ai.session_id.is_empty() { m.insert("session_id".into(), serde_json::json!(ai.session_id)); }
    if !ai.spawned_by.is_empty() { m.insert("spawned_by".into(), serde_json::json!(ai.spawned_by)); }
    if let Some(ref ts) = ai.started_at { m.insert("started_at".into(), serde_json::json!(format!("{}s", ts.seconds))); }
    if let Some(ref md) = ai.metadata { m.insert("metadata".into(), struct_to_json(md)); }
    serde_json::Value::Object(m)
}

fn workspace_to_json(ws: &Workspace) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("name".into(), serde_json::json!(ws.name));
    m.insert("project".into(), serde_json::json!(ws.project));
    if !ws.dir.is_empty() { m.insert("dir".into(), serde_json::json!(ws.dir)); }
    m.insert("status".into(), serde_json::json!(ws.status));
    m.insert("agents".into(), serde_json::Value::Array(ws.agents.iter().map(agent_instance_to_json).collect()));
    if let Some(ref ts) = ws.created_at { m.insert("created_at".into(), serde_json::json!(format!("{}s", ts.seconds))); }
    if let Some(ref md) = ws.metadata { m.insert("metadata".into(), struct_to_json(md)); }
    serde_json::Value::Object(m)
}

fn project_to_json(p: &Project) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("name".into(), serde_json::json!(p.name));
    m.insert("repo".into(), serde_json::json!(p.repo));
    if !p.branch.is_empty() { m.insert("branch".into(), serde_json::json!(p.branch)); }
    let agents: Vec<serde_json::Value> = p.agents.iter().map(|t| serde_json::json!({"name": t.name, "command": t.command})).collect();
    if !agents.is_empty() { m.insert("agents".into(), serde_json::Value::Array(agents)); }
    serde_json::Value::Object(m)
}

fn json_result(val: serde_json::Value) -> sdk::ToolResult {
    sdk::raw_result(serde_json::to_string(&val).unwrap_or_else(|_| "{}".into()))
}

// ---------------------------------------------------------------------------
// Arg helpers
// ---------------------------------------------------------------------------

fn require_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Result<String, sdk::ToolResult> {
    let v = sdk::arg_str(args, key);
    if v.is_empty() { return Err(sdk::err_result(&format!("{key} is required"))); }
    Ok(v)
}

fn parse_json_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Result<Option<serde_json::Value>, sdk::ToolResult> {
    let v = sdk::arg_str(args, key);
    if v.is_empty() { return Ok(None); }
    serde_json::from_str(&v).map(Some).map_err(|e| sdk::err_result(&format!("invalid JSON for {key}: {e}")))
}

fn encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => { out.push('%'); out.push_str(&format!("{:02X}", b)); }
        }
    }
    out
}

fn gen_message_id() -> String {
    static COUNTER: Mutex<u64> = Mutex::new(0);
    let mut c = COUNTER.lock().unwrap();
    *c += 1;
    format!("swb-{:016x}", *c)
}

// ---------------------------------------------------------------------------
// ProjectService — arp.v1.ProjectService
// ---------------------------------------------------------------------------

const PROJECT_SVC: &str = "arp.v1.ProjectService";

fn project_list(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let bytes = match grpc_call(PROJECT_SVC, "ListProjects", &ListProjectsRequest {}.encode_to_vec()) {
        Ok(b) => b, Err(e) => return sdk::err_result(&e),
    };
    if bytes.is_empty() { return json_result(serde_json::json!({"projects": []})); }
    match ListProjectsResponse::decode(&bytes[..]) {
        Ok(r) => json_result(serde_json::json!({"projects": r.projects.iter().map(project_to_json).collect::<Vec<_>>()})),
        Err(e) => sdk::err_result(&format!("decode: {e}")),
    }
}

fn project_register(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let repo = match require_arg(&args, "repo") { Ok(v) => v, Err(r) => return r };
    let mut req = RegisterProjectRequest { name, repo, branch: sdk::arg_str(&args, "branch"), agents: Vec::new() };

    if let Ok(Some(aj)) = parse_json_arg(&args, "agents") {
        if let Some(arr) = aj.as_array() {
            for a in arr {
                let mut t = AgentTemplate::default();
                if let Some(n) = a.get("name").and_then(|v| v.as_str()) { t.name = n.into(); }
                if let Some(c) = a.get("command").and_then(|v| v.as_str()) { t.command = c.into(); }
                if let Some(p) = a.get("port_env").and_then(|v| v.as_str()) { t.port_env = p.into(); }
                if let Some(caps) = a.get("capabilities").and_then(|v| v.as_array()) {
                    t.capabilities = caps.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect();
                }
                req.agents.push(t);
            }
        }
    } else if let Err(r) = parse_json_arg(&args, "agents") { return r; }

    match grpc_call(PROJECT_SVC, "RegisterProject", &req.encode_to_vec()) {
        Ok(b) => match Project::decode(&b[..]) {
            Ok(p) => json_result(project_to_json(&p)),
            Err(e) => sdk::err_result(&format!("decode: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn project_unregister(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    match grpc_call(PROJECT_SVC, "UnregisterProject", &UnregisterProjectRequest { name }.encode_to_vec()) {
        Ok(_) => json_result(serde_json::json!({"status": "success"})),
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// WorkspaceService — arp.v1.WorkspaceService
// ---------------------------------------------------------------------------

const WORKSPACE_SVC: &str = "arp.v1.WorkspaceService";

fn workspace_create(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let project = match require_arg(&args, "project") { Ok(v) => v, Err(r) => return r };
    let mut req = CreateWorkspaceRequest { name, project, branch: sdk::arg_str(&args, "branch"), auto_agents: Vec::new() };
    if let Ok(Some(aa)) = parse_json_arg(&args, "auto_agents") {
        if let Some(arr) = aa.as_array() { req.auto_agents = arr.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect(); }
    } else if let Err(r) = parse_json_arg(&args, "auto_agents") { return r; }
    match grpc_call(WORKSPACE_SVC, "CreateWorkspace", &req.encode_to_vec()) {
        Ok(b) => match Workspace::decode(&b[..]) { Ok(ws) => json_result(workspace_to_json(&ws)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_list(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let st = sdk::arg_str(&args, "status");
    let status = match st.as_str() { "active" => 1, "inactive" => 2, _ => 0 };
    let req = ListWorkspacesRequest { project: sdk::arg_str(&args, "project"), status };
    match grpc_call(WORKSPACE_SVC, "ListWorkspaces", &req.encode_to_vec()) {
        Ok(b) => {
            if b.is_empty() { return json_result(serde_json::json!({"workspaces": []})); }
            match ListWorkspacesResponse::decode(&b[..]) {
                Ok(r) => json_result(serde_json::json!({"workspaces": r.workspaces.iter().map(workspace_to_json).collect::<Vec<_>>()})),
                Err(e) => sdk::err_result(&format!("decode: {e}")),
            }
        }
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_get(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    match grpc_call(WORKSPACE_SVC, "GetWorkspace", &GetWorkspaceRequest { name }.encode_to_vec()) {
        Ok(b) => match Workspace::decode(&b[..]) { Ok(ws) => json_result(workspace_to_json(&ws)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn workspace_destroy(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let name = match require_arg(&args, "name") { Ok(v) => v, Err(r) => return r };
    let req = DestroyWorkspaceRequest { name, keep_worktree: sdk::arg_bool(&args, "keep_worktree").unwrap_or(false) };
    match grpc_call(WORKSPACE_SVC, "DestroyWorkspace", &req.encode_to_vec()) {
        Ok(_) => json_result(serde_json::json!({"status": "success"})),
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// AgentService — arp.v1.AgentService
// ---------------------------------------------------------------------------

const AGENT_SVC: &str = "arp.v1.AgentService";

fn agent_spawn(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let workspace = match require_arg(&args, "workspace") { Ok(v) => v, Err(r) => return r };
    let template = match require_arg(&args, "template") { Ok(v) => v, Err(r) => return r };
    let mut req = SpawnAgentRequest { workspace, template, name: sdk::arg_str(&args, "name"), env: HashMap::new(), prompt: sdk::arg_str(&args, "prompt"), scope: None, permission: 0 };
    if let Ok(Some(ej)) = parse_json_arg(&args, "env") {
        if let Some(obj) = ej.as_object() { for (k, v) in obj { if let Some(s) = v.as_str() { req.env.insert(k.clone(), s.into()); } } }
    } else if let Err(r) = parse_json_arg(&args, "env") { return r; }
    if let Ok(Some(sj)) = parse_json_arg(&args, "scope") {
        let mut scope = Scope::default();
        if let Some(g) = sj.get("global").and_then(|v| v.as_bool()) { scope.global = g; }
        if let Some(ps) = sj.get("projects").and_then(|v| v.as_array()) { scope.projects = ps.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect(); }
        req.scope = Some(scope);
    } else if let Err(r) = parse_json_arg(&args, "scope") { return r; }
    let ps = sdk::arg_str(&args, "permission");
    req.permission = match ps.as_str() { "session" | "PERMISSION_SESSION" => 1, "project" | "PERMISSION_PROJECT" => 2, "admin" | "PERMISSION_ADMIN" => 3, _ => 0 };
    match grpc_call(AGENT_SVC, "SpawnAgent", &req.encode_to_vec()) {
        Ok(b) => match AgentInstance::decode(&b[..]) { Ok(ai) => json_result(agent_instance_to_json(&ai)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_list(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let ss = sdk::arg_str(&args, "status");
    let status = match ss.as_str() { "starting" => 1, "ready" => 2, "busy" => 3, "error" => 4, "stopping" => 5, "stopped" => 6, _ => 0 };
    let req = ListAgentsRequest { workspace: sdk::arg_str(&args, "workspace"), status, template: sdk::arg_str(&args, "template") };
    match grpc_call(AGENT_SVC, "ListAgents", &req.encode_to_vec()) {
        Ok(b) => {
            if b.is_empty() { return json_result(serde_json::json!({"agents": []})); }
            match ListAgentsResponse::decode(&b[..]) {
                Ok(r) => json_result(serde_json::json!({"agents": r.agents.iter().map(agent_instance_to_json).collect::<Vec<_>>()})),
                Err(e) => sdk::err_result(&format!("decode: {e}")),
            }
        }
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_status(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match grpc_call(AGENT_SVC, "GetAgentStatus", &GetAgentStatusRequest { agent_id }.encode_to_vec()) {
        Ok(b) => match AgentInstance::decode(&b[..]) { Ok(ai) => json_result(agent_instance_to_json(&ai)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_stop(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let grace = sdk::arg_str(&args, "grace_period_ms").parse::<i32>().unwrap_or(0);
    let req = StopAgentRequest { agent_id, grace_period_ms: grace };
    match grpc_call(AGENT_SVC, "StopAgent", &req.encode_to_vec()) {
        Ok(b) => match AgentInstance::decode(&b[..]) { Ok(ai) => json_result(agent_instance_to_json(&ai)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_restart(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match grpc_call(AGENT_SVC, "RestartAgent", &RestartAgentRequest { agent_id }.encode_to_vec()) {
        Ok(b) => match AgentInstance::decode(&b[..]) { Ok(ai) => json_result(agent_instance_to_json(&ai)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let req = SendAgentMessageRequest { agent_id, message, context_id: sdk::arg_str(&args, "context_id"), blocking: sdk::arg_bool(&args, "blocking").unwrap_or(true) };
    match grpc_call(AGENT_SVC, "SendAgentMessage", &req.encode_to_vec()) {
        Ok(b) => match SendAgentMessageResponse::decode(&b[..]) {
            Ok(r) => {
                let val = match r.result {
                    Some(send_agent_message_response::Result::Task(s)) => serde_json::json!({"task": struct_to_json(&s)}),
                    Some(send_agent_message_response::Result::Message(s)) => serde_json::json!({"message": struct_to_json(&s)}),
                    None => serde_json::json!({"status": "sent"}),
                };
                json_result(val)
            }
            Err(e) => sdk::err_result(&format!("decode: {e}")),
        },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let message = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let req = CreateAgentTaskRequest { agent_id, message, context_id: sdk::arg_str(&args, "context_id") };
    match grpc_call(AGENT_SVC, "CreateAgentTask", &req.encode_to_vec()) {
        Ok(b) => match prost_types::Struct::decode(&b[..]) { Ok(s) => json_result(struct_to_json(&s)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

fn agent_task_status(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let agent_id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let task_id = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    let hl = sdk::arg_str(&args, "history_length").parse::<i32>().unwrap_or(0);
    let req = GetAgentTaskStatusRequest { agent_id, task_id, history_length: hl };
    match grpc_call(AGENT_SVC, "GetAgentTaskStatus", &req.encode_to_vec()) {
        Ok(b) => match prost_types::Struct::decode(&b[..]) { Ok(s) => json_result(struct_to_json(&s)), Err(e) => sdk::err_result(&format!("decode: {e}")) },
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// DiscoveryService — arp.v1.DiscoveryService
// ---------------------------------------------------------------------------

const DISCOVERY_SVC: &str = "arp.v1.DiscoveryService";

fn discover(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let ss = sdk::arg_str(&args, "scope");
    let scope = match ss.as_str() { "local" | "DISCOVERY_SCOPE_LOCAL" => 1, "network" | "DISCOVERY_SCOPE_NETWORK" => 2, _ => 0 };
    let mut urls = Vec::new();
    if let Ok(Some(uj)) = parse_json_arg(&args, "urls") {
        if let Some(arr) = uj.as_array() { urls = arr.iter().filter_map(|v| v.as_str().map(|s| s.into())).collect(); }
    } else if let Err(r) = parse_json_arg(&args, "urls") { return r; }
    let req = DiscoverAgentsRequest { scope, capability: sdk::arg_str(&args, "capability"), urls };
    match grpc_call(DISCOVERY_SVC, "DiscoverAgents", &req.encode_to_vec()) {
        Ok(b) => {
            if b.is_empty() { return json_result(serde_json::json!({"agent_cards": []})); }
            match DiscoverAgentsResponse::decode(&b[..]) {
                Ok(r) => json_result(serde_json::json!({"agent_cards": r.agent_cards.iter().map(struct_to_json).collect::<Vec<_>>()})),
                Err(e) => sdk::err_result(&format!("decode: {e}")),
            }
        }
        Err(e) => sdk::err_result(&e),
    }
}

// ---------------------------------------------------------------------------
// A2A proxy — HTTP/JSON on port 9099 (/a2a/ endpoints)
// ---------------------------------------------------------------------------

fn a2a_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    let tok = token();
    if !tok.is_empty() { h.insert("Authorization".into(), format!("Bearer {}", tok)); }
    h.insert("Content-Type".into(), "application/json".into());
    h
}

fn a2a_get(path: &str) -> Result<String, String> {
    let req = sdk::HttpRequest { method: "GET".into(), url: format!("{}{}", base_url(), path), headers: a2a_headers(), body: String::new(), body_base64: String::new() };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 { return Err(format!("A2A error ({}): {}", resp.status, resp.body)); }
    if resp.body.trim().is_empty() { return Ok("{}".into()); }
    Ok(resp.body)
}

fn a2a_post(path: &str, body: &str) -> Result<String, String> {
    let req = sdk::HttpRequest { method: "POST".into(), url: format!("{}{}", base_url(), path), headers: a2a_headers(), body: body.to_string(), body_base64: String::new() };
    let resp = sdk::host_http_request(&req)?;
    if resp.status >= 400 { return Err(format!("A2A error ({}): {}", resp.status, resp.body)); }
    if resp.body.trim().is_empty() { return Ok("{}".into()); }
    Ok(resp.body)
}

fn proxy_list(_args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    match a2a_get("/a2a/agents") { Ok(d) => sdk::raw_result(d), Err(e) => sdk::err_result(&e) }
}

fn agent_card(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    match a2a_get(&format!("/a2a/agents/{}/.well-known/agent-card.json", encode_path(&id))) { Ok(d) => sdk::raw_result(d), Err(e) => sdk::err_result(&e) }
}

fn proxy_send_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let msg_text = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let mid = { let v = sdk::arg_str(&args, "message_id"); if v.is_empty() { gen_message_id() } else { v } };
    let mut msg = serde_json::json!({"role": "ROLE_USER", "parts": [{"text": msg_text}], "message_id": mid});
    let cid = sdk::arg_str(&args, "context_id");
    if !cid.is_empty() { msg["context_id"] = serde_json::Value::String(cid); }
    match a2a_post(&format!("/a2a/agents/{}/message:send", encode_path(&id)), &serde_json::json!({"message": msg}).to_string()) {
        Ok(d) => sdk::raw_result(d), Err(e) => sdk::err_result(&e),
    }
}

fn proxy_get_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let tid = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    match a2a_get(&format!("/a2a/agents/{}/tasks/{}", encode_path(&id), encode_path(&tid))) { Ok(d) => sdk::raw_result(d), Err(e) => sdk::err_result(&e) }
}

fn proxy_cancel_task(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let id = match require_arg(&args, "agent_id") { Ok(v) => v, Err(r) => return r };
    let tid = match require_arg(&args, "task_id") { Ok(v) => v, Err(r) => return r };
    match a2a_post(&format!("/a2a/agents/{}/tasks/{}:cancel", encode_path(&id), encode_path(&tid)), "{}") { Ok(d) => sdk::raw_result(d), Err(e) => sdk::err_result(&e) }
}

fn route_message(args: HashMap<String, serde_json::Value>) -> sdk::ToolResult {
    let msg_text = match require_arg(&args, "message") { Ok(v) => v, Err(r) => return r };
    let tags = match parse_json_arg(&args, "tags") { Ok(Some(v)) => v, Ok(None) => return sdk::err_result("tags is required"), Err(r) => return r };
    let mid = { let v = sdk::arg_str(&args, "message_id"); if v.is_empty() { gen_message_id() } else { v } };
    let mut msg = serde_json::json!({"role": "ROLE_USER", "parts": [{"text": msg_text}], "message_id": mid});
    let cid = sdk::arg_str(&args, "context_id");
    if !cid.is_empty() { msg["context_id"] = serde_json::Value::String(cid); }
    match a2a_post("/a2a/route/message:send", &serde_json::json!({"message": msg, "routing": {"tags": tags}}).to_string()) {
        Ok(d) => sdk::raw_result(d), Err(e) => sdk::err_result(&e),
    }
}
