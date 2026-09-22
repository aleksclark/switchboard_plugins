# switchboard_plugins

Community WASM plugins for [Switchboard](https://github.com/daltoniam/switchboard) MCP server.

## Plugins

| Plugin | Tools | Description |
|--------|-------|-------------|
| [homeassistant](plugins/homeassistant/) | 26 | Home Assistant smart home — states, services, events, history, automations, scenes, scripts |
| [radarr](plugins/radarr/) | 22 | Radarr movie manager — movies, calendar, queue, history, commands, quality profiles |
| [sonarr](plugins/sonarr/) | 24 | Sonarr TV series manager — series, episodes, calendar, queue, history, commands |
| [lidarr](plugins/lidarr/) | 27 | Lidarr music manager — artists, albums, calendar, queue, history, quality/metadata profiles |
| [readarr](plugins/readarr/) | 29 | Readarr book manager — authors, books, calendar, queue, history, quality/metadata profiles |
| [prowlarr](plugins/prowlarr/) | 25 | Prowlarr indexer manager — indexers, search, applications, download clients |
| [primer](plugins/primer/) | 39 | Primer TV catalog, Tasks assignments, and content ingest |

## Installing

In the Switchboard web UI (`http://localhost:3847/plugins`):

1. Click **Add Manifest Source**
2. Enter: `https://raw.githubusercontent.com/aleksclark/switchboard_plugins/main/manifest.json`
3. Browse and install plugins with one click

Or install directly by URL:
```
https://github.com/aleksclark/switchboard_plugins/raw/main/dist/homeassistant.wasm
```

## Primer authentication

Use [Primer's native OAuth setup](plugins/primer/README.md) with a Switchboard
build containing native WASM OAuth support. Switchboard handles Clerk PKCE login,
refresh tokens, and token renewal itself; a browser is needed only for initial
consent or reauthorization. No companion process, CDP, or open browser is needed
for tool calls.

Primer TV and content ingest use `tv_base_url` and `tv_admin_key`. Tasks connects
directly to its production API. OAuth setup fields appear in the plugin form;
leave `tasks_api_key` blank in OAuth mode because the host supplies the access
token. A separately issued Tasks service key remains an alternative when the
server supports it. The former browser-auth companion is retired.

## Building from source

```bash
# Requires Rust with wasm32-wasip1 target
rustup target add wasm32-wasip1

# Build all plugins
cargo build --target wasm32-wasip1 --release

# Output at target/wasm32-wasip1/release/<name>_wasm.wasm
```

## Plugin ABI

Plugins target Switchboard ABI v1. Each `.wasm` binary must export:

| Export | Signature |
|--------|-----------|
| `name()` | `-> ptr_size` |
| `metadata()` | `-> ptr_size` |
| `tools()` | `-> ptr_size` |
| `configure(ptr_size)` | `-> ptr_size` |
| `execute(ptr_size)` | `-> ptr_size` |
| `healthy()` | `-> i32` |

The guest SDK ([`switchboard-guest-sdk`](https://github.com/daltoniam/switchboard/tree/main/wasm/guest-rust/sdk)) is sourced from the main Switchboard repository and handles memory management, host function imports, and type serialization.

## Manifest format

The `manifest.json` follows the [Switchboard plugin manifest spec](https://github.com/daltoniam/switchboard/blob/main/docs/plugin-marketplace.md):

```json
{
  "schema_version": 1,
  "name": "registry-name",
  "plugins": [{
    "name": "plugin-name",
    "versions": [{
      "version": "0.1.0",
      "abi_min": 1, "abi_max": 1,
      "url": "https://..../plugin.wasm",
      "sha256": "..."
    }]
  }]
}
```

## License

MIT
