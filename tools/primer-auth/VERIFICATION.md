# Live acceptance, 2026-09-10

## Deployment and authentication decision

- Primer Forgejo master inspected: `68c7575de4c25466aab8d9b497238c3319b316d4`.
- Live Nomad `primer-tasks` version 4 still selected image `ghcr.io/aleksclark/primer-tasks@sha256:5c2eafd20ce49100f5408aaf7d05f674f6b5e1e759d477242acb41e854454755`.
- The already-issued Tasks service key returned HTTP 401 from `/tasks/api/students`; no further migration, issuance, or production deployment was performed.
- The managed Chromium session signed in successfully through the existing Google account. Clerk and Tasks `/auth/session` verified the intended user and Clark School tenant before any companion operation.
- Tasks production accepts Clerk human session JWTs, not Primer Identity OAuth authorization-code/refresh tokens. Copying a session JWT into configuration would expire. Generic OAuth/OIDC support in another Primer service does not authorize access to Tasks.
- The Switchboard-side companion therefore uses Clerk's normal `session.getToken()` refresh in the existing browser. This is interactive session delegation, not an OAuth authorization server or an unattended service credential.

## Real end-to-end checks

Built the actual Primer WASM and companion. Started an isolated Switchboard with a temporary HOME and a randomly allocated nondefault loopback port. Its configuration contained only Primer's required credentials, not a copy of the host config. Backends were real production Primer, not fixture servers.

Both isolated and host Switchboard discovered each tool through MCP `search` before `execute`. Numeric `limit: 1` was checked in every returned page:

| Tool | Result |
| --- | --- |
| `primer_list_students` | 200, one household student |
| `primer_list_tasks` | 200, one of nine task revisions before mutation test |
| `primer_list_task_schedules` | 200, one of three schedules |
| `primer_list_occurrences` | 200, one of 107 occurrences |
| `primer_list_media_items` | 200, one of 5,789 media items |
| `primer_list_content_manifest_entries` | 200, one of 35 ingest manifest entries |

Created one clearly labeled, unassigned draft using `primer_create_task`. Retired that same template using `primer_retire_task`, verified its retired state using `primer_list_tasks`, and observed correct 204 normalization. No student assignment, occurrence decision, media acquisition, or TV mutation was performed. The retired test template remains as an audit record.

Live rejection checks passed: missing local bearer 401, foreign Origin 403, unexpected Host 403, and `/auth/session` proxy attempt 404. Account/tenant mismatch, expired execution, lost-response replay, timeout, and traversal cases are covered by offline tests rather than changing the real user's identity.

Host plugin was uploaded and configured through Switchboard's scoped APIs, which preserved the other 62 integration configurations and registered startup persistence. No host restart was required. Installed WASM SHA-256: `666746a3ef3146a8159218f9d0ce54c6d93e0e7d31e99eae8ff2f7b0b6174327`. Auto-update is disabled for this uploaded artifact.

Companion runs through the repository's `primer-auth` Paseo script. Configuration is private, outside Git. The local bearer is not a Clerk token. Clerk JWTs are acquired and used inside Chromium and are never returned by the companion.

## Reproducible local gates

- `cargo test -p primer-wasm`
- `cargo fmt -p primer-wasm --check`
- `cargo build --target wasm32-wasip1 --release`
- In `tools/primer-auth`: `go test -race ./...`, `go vet ./...`, `go mod verify`.

## Remaining operational dependency

Keep the managed browser connected with exactly one signed-in Tasks tab. Browser sign-out, changed user/household, transport failure, or ambiguous write outcomes fail closed. Recover the session and inspect any uncertain write before restarting the Paseo script. `/health` reports listener availability only. The companion is a working local integration, not a claim that service-key authentication has been deployed or that unattended access is supported.
