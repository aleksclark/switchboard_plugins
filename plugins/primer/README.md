# Primer

40 tools for Primer TV catalog/programming, Tasks assignments and verification,
and content ingest manifests. The plugin uses Switchboard's HTTP host capability.
Content ingest tools manage the TV-side manifest and catalog; they do not run the
batch acquisition CLI.

## Discover TV devices for viewing tasks

Use `primer_list_tv_devices` to find a playback target, then
`primer_get_tv_device` with its `id` to recheck the selection. The list tool
already existed; this update adds assignment-focused discovery guidance, a
single-device lookup and credential-safe results rather than a second list alias.
If keyword search misses it, enumerate the `primer` integration or search for the
exact tool name. A running host needs the updated WASM installed/reloaded to
expose the new lookup; opening or merging a PR does not update the running host.

Both tools are read-only and use the existing `tv_base_url` plus `tv_admin_key`
against TV's `/devices` boundary. They never use a Tasks OAuth token for TV, alter
pairing, or expose pairing codes, pairing expiry, token hashes or future unknown
fields. API/transport errors are reported without echoing response bodies.

List arguments: `limit` (1-200, default 20), `offset` (0-1,000,000, default 0),
`q` (device name, at most 200 characters), `sort`, `dir`, and optional
`filter: "kind:tv_box"` or `"kind:tablet"`. Results retain the server's
`totalCount`, `limit`, and `offset`; unpaired/revoked records are not silently
filtered out, so counts and pagination remain honest. Follow all pages when
looking for a device.

Each item exposes only `id`, `name`, `kind`, optional `pairedAt`, `revokedAt`,
`lastSeenAt`, and derived `assignmentReady`. Readiness requires a nonempty pairing
timestamp, no revocation, and an empty outstanding pairing code. Missing pairing
state is not assumed ready. This is a discovery snapshot, not authority to assign:
Tasks revalidates the selection against its own TV integration and household.
The TV administrator credential is scoped to that TV service, not a Tasks household.

Viewing-task workflow:

1. Discover Amos (or another student) with `primer_list_students`.
2. Find the exact episode using `primer_list_media_items`.
3. List/recheck the intended playback device; require `assignmentReady: true`.
4. Use the returned `id` as `config.deviceId` and `name` as `config.deviceName`
   in `primer_create_task` with `kind: "video_watch"`, `interaction: "external"`,
   `executor: "primertv"`, and catalog `mediaId`, `title`, `runtimeSeconds`.
5. Publish with `primer_publish_task`, then assign with
   `primer_create_task_schedule`. Structured `body` arguments are JSON strings.

Verification commands from the repository root:

```sh
cargo test -p primer-wasm
cargo fmt -p primer-wasm --check
RUSTFLAGS='-C link-arg=--allow-undefined' cargo build --target wasm32-wasip1 --release
go -C tools/wasm-debug test -race ./...
```

The artifact smoke test exercises the actual exported tool registry, dispatcher,
TV authorization header, read-only routes, paging and credential/error redaction
against a local HTTP fixture. It requires the built WASM and does not skip when
it is missing. No live credentials or production device changes are needed.

## Native Clerk OAuth

Requires a Switchboard build containing [native WASM OAuth support](https://github.com/daltoniam/switchboard/pull/234)
and a Tasks deployment accepting delegated OAuth for the registered client.
Switchboard performs authorization-code S256 PKCE, verifies the expected user,
stores tokens privately, and refreshes them natively. Browser use ends after
initial consent; no companion or browser automation is involved in requests.

1. Install the Primer WASM in the host Switchboard plugin marketplace.
2. Open `http://127.0.0.1:3847/integrations/primer` (use `127.0.0.1`, not `localhost`).
3. Fill the following configuration. Placeholder text is guidance, not a default.

| Field | Value |
| --- | --- |
| `tv_base_url` | Your TV API base including `/api/v1` |
| `tv_admin_key` | TV administrator API key, separate from Tasks credentials |
| `tasks_base_url` | `https://api.primerlms.com/tasks/api` |
| `tasks_api_key` | Leave blank for OAuth; supplied to the plugin by Switchboard |
| `oauth_issuer` | `https://clerk.primerlms.com` |
| `oauth_client_id` | The public Clerk OAuth application's client ID |
| `oauth_scopes` | `openid profile email offline_access tasks:read tasks:write` |
| `oauth_token_key` | `tasks_api_key` |
| `oauth_subject` | Your exact Clerk `user_...` identifier |
| `oauth_email` | Your expected Clerk account's primary email |

4. Save settings so Connect OAuth becomes visible if this is a new installation.
5. Click **Connect OAuth**, sign into the intended account, and approve the task
   read/manage permissions. Wait for the redirect back to Switchboard.
6. Enable the integration if it was disabled. Connecting alone does not enable it.
7. Search for `primer_list_students` and execute it with `limit: 1` to verify.

Register the exact host callback in the public Clerk application:

```text
http://127.0.0.1:3847/api/integrations/primer/oauth/callback
```

Use the host's actual port for both setup and callback. A temporary smoke server
has its own port/configuration; approving it does not configure the host server.
Never paste a Clerk session JWT into `tasks_api_key`: it expires and is not an
OAuth refresh credential. Never copy access/refresh tokens into the plugin form.
The host owns those fields and passes only the current access token to the guest.

The registered client must be explicitly enabled by Tasks through
`TASKS_OAUTH_CLIENT_IDS`. Task scopes authorize specific operations, while the
verified issuer/user subject must map to an existing active household admin.
Email and Clerk organization names do not create or select household authority.

## Service-key alternative

For a Tasks deployment supporting operator-issued tenant service keys, leave all
`oauth_*` fields empty and provide the key in `tasks_api_key`. The key is still
required at plugin execution time even though the setup form marks the field
optional to support host-managed OAuth.

## Troubleshooting

- **No Connect OAuth button:** save a nonempty `oauth_issuer`; ensure the host has native WASM OAuth support.
- **Authorization required:** finish consent in the same browser that started it. Check the registered callback and exact expected identity.
- **403 scope denied:** reconnect requesting `tasks:read` and `tasks:write`; an existing identity-only grant cannot be silently expanded.
- **401 parent session required:** the deployed Tasks API does not yet accept OAuth access tokens. Deploy the delegated boundary; do not substitute an ID token or fabricate a session ID.
- **TV authorization failure:** check the separate TV key/base URL. OAuth for Tasks does not grant TV access.
- **Refresh persistence error:** repair host configuration storage before restarting; a rotated refresh token may only exist in host memory until saving succeeds.

The former `tools/primer-auth` companion is historical and unsupported. Stop any
old companion script, replace its loopback Tasks URL with the direct production
URL, clear its local bearer, and configure native OAuth instead.
