# Agents Plugin

Switchboard WASM plugin for the ARP agent registry and A2A agent management.

Manages projects, workspaces, spawns/stops/restarts agents, sends messages,
discovers agents, and routes by skill via the ARP gRPC/HTTP API.

## Configuration

| Key | Required | Description |
|-----|----------|-------------|
| `base_url` | **yes** | ARP server URL (e.g. `http://localhost:9099`) |
| `token` | no | Bearer token for authentication (optional for localhost) |
| `h2c` | no | Enable HTTP/2 cleartext (h2c) transport. Default: `false` |

### h2c mode

ARP exposes two ports:

- **9099** — HTTP/1.1 gateway (default). Serves `/a2a/*` routes and (when
  available) the `/v1/*` gRPC-Web transcoding routes.
- **9098** — gRPC/h2c. Always serves `/v1/*` routes via gRPC-Web transcoding.

Set `h2c=true` when you want the plugin to talk to the gRPC/h2c port (9098).
When enabled, all HTTP requests include an `X-H2C: 1` header that tells the
Switchboard host to upgrade the connection to HTTP/2 cleartext instead of
using HTTP/1.1.

**Typical configurations:**

```
# Default: HTTP/1.1 to the gateway port
base_url = http://localhost:9099
h2c = false

# gRPC/h2c to the gRPC port (for /v1/* routes)
base_url = http://localhost:9098
h2c = true
```

### Empty response handling

Several ARP endpoints return HTTP 200 with an empty body on success (e.g.
delete operations, stop/restart). The plugin normalizes these to
`{"status":"success"}` so callers always receive valid JSON.

## Tools

### Project management (`/v1/projects`)
- `agents_project_list` — List all registered projects
- `agents_project_register` — Register a new project
- `agents_project_unregister` — Unregister a project

### Workspace management (`/v1/workspaces`)
- `agents_workspace_create` — Create a workspace (git worktree)
- `agents_workspace_list` — List workspaces
- `agents_workspace_get` — Get workspace details
- `agents_workspace_destroy` — Destroy a workspace

### Agent lifecycle (`/v1/agents`)
- `agents_agent_spawn` — Spawn an agent from a template
- `agents_agent_list` — List agent instances
- `agents_agent_status` — Get agent status
- `agents_agent_stop` — Stop an agent
- `agents_agent_restart` — Restart an agent

### Agent messaging (`/v1/agents/{id}/messages|tasks`)
- `agents_agent_message` — Send a message to an agent
- `agents_agent_task` — Create a task on an agent
- `agents_agent_task_status` — Check task status

### Discovery (`/v1/discover`)
- `agents_discover` — Discover agents across workspaces or network

### A2A proxy (`/a2a/`)
- `agents_proxy_list` — List A2A agent cards
- `agents_agent_card` — Get an agent's A2A card
- `agents_proxy_send_message` — Send A2A message via proxy
- `agents_proxy_get_task` — Get A2A task via proxy
- `agents_proxy_cancel_task` — Cancel A2A task via proxy
- `agents_route_message` — Route message by skill tags
