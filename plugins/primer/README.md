# Primer

103 tools for Primer TV catalog/programming, Tasks assignments and verification,
the LMS reading library and reading plans, and content ingest manifests. The
plugin uses Switchboard's HTTP host capability.
Content ingest tools manage the TV-side manifest and catalog; they do not run the
batch acquisition CLI.

## Reading library and reading plans

61 tools manage the LMS reading library and reading plans with the same Clerk
OAuth access token used for Tasks. Set `lms_base_url` to the LMS API base
including `/api/v1` (production: `https://api.primerlms.com/api/v1`). Without it
these tools report that the LMS is not configured and make no request.

Start with `primer_list_reading_works` for books and `primer_list_reading_plans`
for plans. Typical workflow:

1. **Library:** `primer_create_reading_work`, `primer_create_reading_edition`
   (`kind: "source"` for the EPUB, `kind: "print"` for the student's book), then
   `primer_import_reading_epub` from an HTTPS download URL. Poll
   `primer_get_reading_import`, review `primer_get_reading_import_draft`, and
   publish with `primer_publish_reading_revision`. Optional print page maps use
   `primer_create_reading_map` and `primer_publish_reading_map`; remove a wrong map with `primer_delete_reading_map`.
2. **Plan:** find the LMS student with `primer_list_lms_students` (LMS IDs differ
   from Tasks IDs), then `primer_create_quick_reading_plan`, or
   `primer_create_reading_plan` for explicit segments and outcomes.
   `primer_schedule_reading_segment` moves a segment to another day.
3. **Questions:** `primer_prepare_reading_segment`, then
   `primer_generate_reading_questions` or `primer_generate_reading_question_batch`
   (these can bill the question provider), poll `primer_get_reading_question_job`,
   and save the parent's choices with `primer_save_reading_question_draft`.
4. **Assign:** link the LMS and Tasks students once: `primer_link_reading_tasks_student`
   returns a one-time code, `primer_confirm_reading_tasks_link` confirms it in Tasks
   (needs `tasks:write`), and `primer_finalize_reading_tasks_student_link` completes it;
   then `primer_review_reading_assignment` and `primer_publish_reading_assignment`
   with the returned `reviewedHash`. Track it with `primer_get_reading_assignment`
   and `primer_get_reading_academic_review`.

Plans also support adjustments, reinforcements, reading confirmations and
deletion. Structured `body` arguments are JSON strings using the LMS field names
shown in each tool description; versioned updates need the record's current
`version`. Create, publish and generation tools send an `Idempotency-Key`. By
default it is derived from the exact request, so retrying after a timeout replays
the original result instead of creating a duplicate; pass `idempotency_key` to
deliberately repeat an identical request.

`primer_import_reading_epub` downloads the file through the Switchboard host
without Primer credentials, rejects non-ZIP or possibly truncated (10 MiB host
limit) downloads, computes SHA-256, and uploads it with the rights source,
license and acknowledgement. Larger EPUBs must be uploaded in the LMS.

The LMS accepts these tokens only when `LMS_OAUTH_CLIENT_IDS` admits the client
and the token carries `reading:read` (lookups and previews) or `reading:write`
(changes). Your Clerk user must be linked to an LMS parent/admin educator. Worksheet
PDFs, source downloads and table-of-contents image review stay in the LMS UI.

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
| `lms_base_url` | `https://api.primerlms.com/api/v1` for reading library and plan tools |
| `oauth_issuer` | `https://clerk.primerlms.com` |
| `oauth_client_id` | The public Clerk OAuth application's client ID |
| `oauth_scopes` | `openid profile email offline_access tasks:read tasks:write reading:read reading:write` |
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
- **403 scope denied:** reconnect requesting `tasks:read` and `tasks:write` (plus `reading:read` and `reading:write` for reading tools); an existing grant cannot be silently expanded. The scopes must also be assigned to the Clerk OAuth application, or Clerk rejects the request with `invalid_scope`.
- **LMS 401 delegated OAuth credential required:** the deployed LMS does not admit this client yet (`LMS_OAUTH_CLIENT_IDS`), or the token was not an OAuth access token.
- **LMS 403 parent access denied:** link your Clerk user to an LMS parent/admin educator with `lms-parent-link`.
- **401 parent session required:** the deployed Tasks API does not yet accept OAuth access tokens. Deploy the delegated boundary; do not substitute an ID token or fabricate a session ID.
- **TV authorization failure:** check the separate TV key/base URL. OAuth for Tasks does not grant TV access.
- **Refresh persistence error:** repair host configuration storage before restarting; a rotated refresh token may only exist in host memory until saving succeeds.

The former `tools/primer-auth` companion is historical and unsupported. Stop any
old companion script, replace its loopback Tasks URL with the direct production
URL, clear its local bearer, and configure native OAuth instead.
