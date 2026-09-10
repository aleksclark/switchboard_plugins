# Primer browser auth companion

A user-authorized, local Switchboard Tasks companion. This is a separate Go 1.25 module using `github.com/coder/websocket` v1.8.14 for the native CDP client. `golang.org/x/net/websocket` v0.53.0 is used only by fake test servers. The live Tasks deployment currently accepts human Clerk JWTs; merged service-key support is not deployed. This tool uses an existing, explicitly signed-in human browser session. It does not invent OAuth flows, log in, copy JWTs into Go, or persist tokens.

## Requirements and trust boundary

- Linux or a compatible Unix system supporting no-follow file opens and process groups.
- Go 1.25+ to build; `browser-use-terminal` installed and **already connected** to your user-authorized managed browser. Its `browser status --json` must report `endpoint.ws_url` for that browser on literal IPv4 loopback. This tool never starts or connects a managed browser for you.
- Exactly one existing tab whose URL starts with `https://api.primerlms.com/tasks/`. Sign in yourself. Multiple matching tabs fail closed rather than guessing.
- The expected Clerk user must have an active session and the exact configured primary email. Tasks `/auth/session` must return the configured `tenantId` and `subjectRef`.
- Trust the local OS account, browser, installed browser CLI, browser extensions, and Primer application. This is not isolation from malware or compromised browser code. Do not enable browser/CDP network tracing, debug recording, or external HTTP capture of authenticated traffic.

This is **not an unattended service credential**. Access stays active only while the user remains signed in. Normal Clerk `session.getToken()` SDK refresh is used; there are no refresh-token files, credential environment variables, background keep-alives, or automatic sign-in. A missing tab, unloaded Clerk, changed account/session, wrong tenant/subject, or failed identity check fails closed.

## Configure and run

Create the config outside the repository in a private directory. Generate a random local key; length checks cannot prove randomness. This example creates the file atomically with mode `0600` without printing the key:

```sh
mkdir -p "$HOME/.config/primer-auth"
chmod 700 "$HOME/.config/primer-auth"
python3 - <<'PY'
import json
import os
import secrets

path = os.path.expanduser('~/.config/primer-auth/config.json')
config = {
    'local_api_key': secrets.token_urlsafe(32),
    'clerk_user_id': 'user_REPLACE_WITH_YOUR_USER_ID',
    'email': 'parent@example.com',
    'tenant_id': 'REPLACE_WITH_TENANT_ID',
    'parent_subject': 'REPLACE_WITH_PARENT_SUBJECT',
    'port': 8765,
}
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, 'w') as file:
    json.dump(config, file, indent=2)
PY
```

Replace the identity placeholders with your known account details using a trusted local editor. Do not put a Clerk JWT in this file.

| Field | Meaning |
| --- | --- |
| `local_api_key` | Random local bearer key, 32–512 printable non-space ASCII characters. Generate with a cryptographic RNG. |
| `clerk_user_id` | Expected Clerk `user_…` identifier. |
| `email` | Exact primary email on that Clerk user; comparison is case-sensitive. |
| `tenant_id` | Expected Tasks session `tenantId`. |
| `parent_subject` | Expected Tasks session `subjectRef`, not an arbitrary impersonation target. |
| `port` | Integer 1–65535; choose an unused unprivileged port. Binding is always `127.0.0.1`. |
| `browser_command` | Optional executable name or absolute executable path; defaults to `browser-use-terminal`. Not a shell command or argument list. Prefer an absolute path. |

Config must be a regular file, not a symlink, with mode `0600` or stricter. Unknown/duplicate fields, missing required values, unsafe permissions, and oversized files are rejected. The file is read once at startup. Keep its parent directory private; restart to apply changes or rotate the local key. Only `PATH` and `HOME` are inherited for status discovery; credential, proxy, display, and debug environment variables are not forwarded. Discovery runs in a newly created, empty, mode-0700 temporary working directory, not the project directory, so project `.env` files are not loaded. That directory is removed afterward; no request data, scripts, endpoints, or tokens are written there.

From this directory:

```sh
go build -o primer-auth .
./primer-auth -config "$HOME/.config/primer-auth/config.json"
```

Configure the existing Primer plugin:

- `tasks_base_url`: `http://127.0.0.1:8765` (no `/tasks/api` suffix).
- `tasks_api_key`: the generated **local** key from the private config, not a Clerk JWT or a Tasks service key.
- Keep the plugin's unrelated TV configuration unchanged.

The caller must run on the same host/network namespace. Do not expose this through a reverse proxy, container port mapping, network tunnel, or public listener. The local key authorizes all 16 approved operations; only give it to a trusted local Switchboard instance. Configure callers not to retry mutations automatically.

`GET http://127.0.0.1:8765/health` is public, static, and read-only for Paseo. It returns `{"ready":true}` without invoking a browser or upstream API. It reports only that the local HTTP handler is available, **not** browser sign-in or upstream readiness. It still requires the literal Host and rejects Origin headers and bodies.

## Allowed HTTP surface

All other requests require exactly `Authorization: Bearer <local_api_key>`. Authentication precedes route/body validation. Host must be exactly `127.0.0.1:<configured port>`; `localhost`, IPv6, foreign hosts, Origin headers (including empty Origin), CORS/preflight, upgrades, and absolute-form request targets are rejected. No incoming headers or cookies are forwarded.

| Method | Route | Body / query |
| --- | --- | --- |
| GET | `/students` | Standard list filters |
| GET | `/tasks` | Task list filters |
| POST | `/tasks` | Required JSON object |
| POST | `/tasks/{UUID}/revisions` | Required JSON object |
| POST | `/tasks/{UUID}/publish` | Empty body or `{}`; sends `{}` |
| POST | `/tasks/{UUID}/retire` | Empty body or `{}`; sends `{}` |
| GET | `/schedules` | Standard list filters |
| POST | `/schedules` | Required JSON object |
| PATCH | `/schedules/{UUID}` | Required JSON object |
| DELETE | `/schedules/{UUID}` | No body or query |
| GET | `/occurrences` | Standard list filters |
| GET | `/occurrences/{UUID}` | No body or query |
| POST | `/occurrences/{UUID}/decision` | Required JSON object |
| POST | `/occurrences/{UUID}/retry` | No body; optional `requirementId`, `attemptId` UUID queries |
| POST | `/occurrences/{UUID}/skip` | Empty body or `{}`; sends `{}` |
| POST | `/occurrences/{UUID}/cancel` | Empty body or `{}`; sends `{}` |

Standard list keys: `limit`, `offset`, `q`, `sort`, `dir`, `filter` (the plugin describes `filter` as `column:value`). Task list keys: `limit`, `offset`, `q`, `sort`, `dir`, `status`, `view`. `view`, when supplied, must be `templates`. `limit` is 1–200, `offset` a nonnegative 31-bit integer, and `dir` is `asc` or `desc`. Omitted options remain omitted, preserving the plugin's upstream list defaults rather than inventing new defaults. Search, sort, filter and status values retain upstream semantics; this is not an independent Tasks schema implementation.

Unknown/duplicate query keys, malformed escapes, controls, excessive query sizes, trailing slashes, traversal, non-UUID identifiers, and **all path percent escapes** are rejected. Only query values may use percent encoding. Request bodies are limited to 256 KiB, must be UTF-8 JSON objects with `Content-Type: application/json` (optional UTF-8 charset), and cannot be compressed. GET bodies are rejected. The plugin's bodyless retry and DELETE behavior is preserved. There is no arbitrary URL, method, header, or path proxy; auth, student detail/mutations, device, management, and websocket endpoints are never exposed. The internal `/auth/session` verification is the sole fixed authentication request.

## Browser execution and failures

### Ephemeral discovery and direct CDP

The **only** subprocess invocation is the configured executable with fixed arguments `browser status --json`, empty stdin, and the minimal environment described above. No shell is used by Go. The CLI is used only for discovery of its already-connected managed browser: **no `browser connect`, `browser exec`, captures, screenshots, or result logging through the CLI**. Neither scripts nor request bodies are passed in argv, stdin, or environment variables.

Discovery reads only the status JSON's `endpoint.ws_url` as the endpoint authority. It must have the form `ws://127.0.0.1:<port>/devtools/browser/<id>` with a valid nonzero port; remote hosts, DNS names (including `localhost`), IPv6, TLS endpoints, userinfo, query strings, fragments, and page endpoints are rejected. The status must explicitly report `connection: "connected"` and `mode: "managed"`. Missing/disconnected/unmanaged states, missing endpoints, ambiguous duplicate fields, malformed JSON, and CLI errors fail closed. The endpoint is ephemeral in-memory data: it is never printed, logged, cached to disk, or persisted by this companion.

Go opens one direct native WebSocket to **that same discovered browser**, sending **no Origin header**. A private HTTP transport uses only the validated IPv4 loopback connection, refuses redirects, and does not inherit HTTP client defaults, proxies, cookies, or authorization headers. There are no alternative endpoints, browser launches, reconnection, or command retries. The command sequence is:

1. `Target.getTargets`: select exactly one existing `page` target with exact `https://api.primerlms.com` origin and `/tasks/` path prefix. No guessing among multiple matches.
2. `Target.attachToTarget` with `flatten: true`.
3. One `Runtime.evaluate` on that target session with `awaitPromise: true` and `returnByValue: true`, carrying the safely JSON-serialized JavaScript template.
4. Best-effort `Target.detachFromTarget` once, then socket close. If a reply is lost or the context canceled, close the socket instead of sending another command while one is outstanding.

There is **no tab activation, `Page.bringToFront`, navigation, target creation, screenshot, network tracing, or domain enabling**. Commands are strictly sequential, replies must match their ID and session, and events are ignored, never logged. Raw CDP errors and exceptions are not returned to callers.

CDP grants broad access to the managed browser, not an isolated Tasks capability. Trust the CLI's status metadata to identify the user-authorized browser, the local OS account and its installation/configuration, the loopback listener, the browser and extensions, and Primer's page/Clerk/fetch implementations. A malicious local process, page, or other CDP client can violate this boundary. Keep remote debugging inaccessible off-host and do not enable external recording. The native CDP handshake omits Origin, matching backend clients and avoiding managed Chrome's rejection of Origin-bearing connections. It never forges a browser Origin or changes Chrome flags such as `--remote-allow-origins`. Other handshake failures still fail closed rather than disabling security or selecting another browser. Offline tests simulate Chrome's rejection of any Origin header; they do not perform a live-browser compatibility check.

### Identity, replay protection, and failure semantics

Every evaluation verifies the exact origin and `/tasks/` path, expected Clerk user ID and exact primary email, active session and session user, and captures that session's ID. It obtains one token and uses the **same token** for the fixed `/tasks/api/auth/session` verification and the approved operation. Verification must match the configured tenant and parent subject. Identity/session/location checks repeat before token acquisition, after acquisition, before operation submission, and after the response. Both fetches use absolute URLs under `https://api.primerlms.com/tasks/api`, `mode: 'same-origin'`, `credentials: 'omit'`, `redirect: 'error'`, and `AbortSignal.timeout(12000)`. Token acquisition is bounded to 2 seconds. The callback returns only `{status, body}`; it never returns the JWT, and it blocks upstream JSON echoing the current token. No tokens are persisted by the companion (Clerk's normal browser session storage remains under Clerk/browser control).

- One request owns the Go browser slot at a time, including body validation. Concurrent approved requests receive `503`; there is no queue. A browser-side ownership guard also rejects overlapping scripts.
- Go embeds a fresh cryptographically random 256-bit `requestID` and an **absolute Unix-millisecond expiry before discovery**. Discovery latency consumes the lifetime. The script rejects expiry before `getToken` and immediately before operation submission; delayed execution cannot reset the deadline. The full Go discovery/handshake/CDP operation has a context deadline of at most 28 seconds, including caller deadlines. Cancellation closes the underlying socket, including during handshake. HTTP header/body read deadlines also bound stalled uploads.
- A browser-global map records request IDs and state **even after completion**, rejecting duplicate evaluation with `409` without another token or fetch. Entries contain no tokens, payloads, or results. Entries have a 60-second retention TTL, are pruned on later valid requests, and are capped at 1,024; capacity exhaustion rejects new work rather than evicting live guards. Expired payloads remain ineligible even after their ID is pruned. This assumes a trustworthy system/browser clock and page globals. Reloading clears the map; this is not durable upstream idempotency and does not deduplicate new HTTP requests with fresh IDs.
- Discovery's combined stdout/stderr is bounded to 2 MiB; stderr is discarded and stdout is parsed only as status JSON. Handshake response headers and WebSocket messages are each bounded to 2 MiB, and the entire connection's incoming traffic (including handshake/events) to 16 MiB. Read/write deadlines never extend the request deadline; detach gets at most 250 ms of the remaining time. Operational logs contain only fixed events, status codes, and the companion's loopback listening address, not the CDP endpoint or request/response content.
- Responses must be JSON, with an upstream body limit of 1 MiB and an additional serialized-output limit. A legitimate HTTP `204` is normalized internally to `{status: 'success'}` without reading a body; Go preserves **HTTP 204 with an empty response body**, allowing the plugin's existing empty-response normalization to apply. Other empty/non-JSON bodies, redirects, invalid UTF-8, nonfinite numbers, and unsafe integer numbers fail closed. Ordinary JSON numbers use JavaScript numeric semantics. Request JSON text is sent without a numeric parse/reserialize round trip.
- Validated results preserve upstream status and JSON body, including ordinary upstream `4xx`/`5xx` responses. Headers are never returned. There are **no automatic retries**, for reads or writes. Fetch aborts, lost replies, identity changes after submission, or timeouts during a write produce a generic error explicitly saying **write outcome unknown**. Inspect state manually before deciding whether another mutation is appropriate.

Closing a CDP socket cannot prove an already-running browser fetch stopped. On discovery/transport failure, cancellation, overflow, invalid results, or a `502` browser result, the runner latches unavailable until restarted. The conservative `502` rule also applies to upstream `502`, because the two-field result intentionally carries no trusted internal provenance. `/health` remains static. Before restarting after such a failure, stop/reload the Tasks tab, re-establish the intended session, and inspect whether any write took effect. Replay/busy guards and expiry prevent duplicate or delayed work within their stated scope; they do not undo a submitted mutation. Closing/reloading a page or losing a connection does not guarantee upstream rollback.

## Validation

```sh
gofmt -w *.go
go test ./...
go test -race ./...
go vet ./...
```

Tests use fake browser runners, status-only subprocesses, and a loopback fake CDP WebSocket server. Transport tests cover no-Origin handshakes against a Chrome-style rejection policy, redirect refusal, loopback-only endpoint validation, isolation from HTTP defaults and proxies, handshake limits/deadlines, exact target selection, no activation/navigation, bounded output, reply matching, cancellation, lost responses without resend, and maximum-size bodies exclusively in WebSocket payloads. When Node.js is installed, offline JavaScript tests cover signed-out/wrong-tenant sessions, changing identities, replay after success executing a mutation once, expiry before execution/submission, 204 normalization, aborts, output bounds, escaping, fixed-origin requests, and write uncertainty. The JavaScript suite skips if Node or required browser primitives are unavailable. Tests never authenticate to Primer or call live Tasks. No token fixtures or real keys are included.
