# AGENTS.md

## Project Overview

Switchboard WASM plugin repository. Plugins are Rust crates compiled to `wasm32-wasip1` that run inside the [Switchboard](https://github.com/daltoniam/switchboard) MCP server. Each plugin exposes tools (via a fixed ABI) that Switchboard makes available to LLM agents.

## Commands

```bash
# Build all plugins (the only build command)
cargo build --target wasm32-wasip1 --release

# Output location
# target/wasm32-wasip1/release/<crate_name>.wasm
# Crate name uses underscores: homeassistant-wasm -> homeassistant_wasm.wasm

# Prerequisite (one-time)
rustup target add wasm32-wasip1
```

There are no tests, lints, or CI pipelines in this repo.

## Architecture

```
Cargo.toml          # Workspace root: members = ["plugins/*"]
plugins/
  <name>/           # One crate per plugin, crate-type = ["cdylib"]
    src/
      lib.rs        # ABI exports + handler logic
      tools.rs      # Tool definitions (names, descriptions, parameters)
manifest.json       # Plugin registry — Switchboard UI fetches this to list installable plugins
```

### SDK Dependency

The `switchboard-guest-sdk` is **not vendored** — it lives in the main [Switchboard repo](https://github.com/daltoniam/switchboard) at `wasm/guest-rust/sdk/`. Plugins reference it as a git dependency:

```toml
switchboard-guest-sdk = { git = "https://github.com/daltoniam/switchboard.git" }
```

Cargo auto-discovers the crate by name within the git repo (no `path` field needed or allowed alongside `git`). The `serde` and `serde_json` deps are declared as `[workspace.dependencies]` in the root `Cargo.toml` and inherited via `.workspace = true`.

### Plugin ABI Contract

Every plugin must export these six `extern "C"` functions (see the [upstream SDK source](https://github.com/daltoniam/switchboard/blob/main/wasm/guest-rust/sdk/src/lib.rs) for types):

| Export | Purpose |
|--------|---------|
| `name()` → `u64` | Plugin name as leaked string |
| `metadata()` → `u64` | JSON-serialized `PluginMetadata` (includes credential_keys, capabilities) |
| `tools()` → `u64` | JSON-serialized `Vec<ToolDefinition>` |
| `configure(u64)` → `u64` | Receives credentials JSON, returns 0 on success or error string |
| `execute(u64)` → `u64` | Receives `ExecuteRequest`, returns JSON-serialized `ToolResult` |
| `healthy()` → `i32` | Health check, 1 = healthy |

All pointer passing uses a packed `u64` = `(ptr << 32) | size`. The SDK's `leaked_result`, `leaked_string`, `read_input` handle this.

### SDK Host Imports

Plugins can call into the Switchboard host via:
- `host_http_request(HttpRequest) -> HttpResponse` — the only way plugins make network calls
- `host_log(msg)` — logging

Plugins declare needed capabilities in `PluginMetadata.capabilities` (e.g., `["http"]`).

### Memory Model

Memory is managed via `guest_malloc`/`guest_free` exports (defined in SDK). All returned data is **leaked** (`std::mem::forget`) — the host is responsible for freeing it via `guest_free`. This is intentional, not a bug.

## Adding a New Plugin

1. Create `plugins/<name>/Cargo.toml` with `crate-type = ["cdylib"]` and `switchboard-guest-sdk` git dependency (see SDK Dependency section above)
2. Create `plugins/<name>/src/lib.rs` with the six ABI exports
3. Create `plugins/<name>/src/tools.rs` with `tool_definitions() -> Vec<ToolDefinition>`
4. Wire a `dispatch()` match table in `lib.rs` mapping tool names to handler functions
5. Add the plugin to `manifest.json` with version, sha256, and size
6. Tool names **must** be prefixed with the plugin name and underscore (e.g., `homeassistant_list_states`)

## Conventions

- **Tool naming**: `<plugin>_<verb>_<noun>` (e.g., `homeassistant_call_service`, `homeassistant_get_history`)
- **Argument extraction**: Use SDK helpers (`arg_str`, `arg_int`, `arg_bool`, `arg_map`) — they handle both typed JSON values and stringified fallbacks
- **Required args**: Validate with a `require_arg` pattern that returns `Err(sdk::ToolResult)` for early exit
- **JSON string args**: When a tool parameter accepts structured data, it's passed as a JSON string (not a nested object). Use `parse_json_arg` to validate and re-serialize
- **HTTP helpers**: Factor out `do_get`/`do_post`/`do_delete` with shared auth headers; check `resp.status >= 400` for errors
- **CRUD pattern**: For resource types with get/save/delete, use shared `crud_get`/`crud_save`/`crud_delete` functions parameterized by ID key name and API path prefix
- **Error results**: Always use `sdk::err_result()` — never panic. The WASM sandbox makes panics unrecoverable
- **Config via global Mutex**: Plugin config is stored in a `static Mutex<Option<Config>>` set by `configure()` and accessed via helper closures
- **PluginMetadata.credential_keys**: List all keys the plugin needs from the user. Mark non-secret ones in `plain_text_keys`. Provide example values in `placeholders`
- **`configure()` returns 0 on success**: Not an empty string — literal integer `0` (the null `u64`)

## manifest.json

When releasing, update `manifest.json` with:
- Correct `sha256` of the `.wasm` file
- Correct `size` in bytes
- Bumped `version`
- Current `released_at` timestamp
- `abi_min`/`abi_max` should stay at `1` unless the Switchboard ABI changes
