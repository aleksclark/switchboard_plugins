# platen

Switchboard plugin for [Platen](https://github.com/giovannirco/platen), a print and scan server for IPP printers and eSCL scanners. It calls Platen's REST API (`/api/v1`), so it needs no MCP-to-MCP proxying.

## Credentials

| Key | Value |
|---|---|
| `base_url` | Platen origin, e.g. `https://print.example.lan` (no `/api` suffix) |
| `token` | One of Platen's `PLATEN_AUTH_TOKENS` (sent as `Authorization: Bearer`) |

## Tools

`platen_list_devices`, `platen_printer_status`, `platen_printer_queue`, `platen_print_document`, `platen_list_print_jobs`, `platen_cancel_print_job`, `platen_scan_document`, `platen_scan_next_page`, `platen_get_scan`, `platen_list_scans`, `platen_rename_scan`, `platen_delete_scan`, `platen_file_scan_in_paperless`, `platen_search_paperless`.

### Paper guard rail

Platen refuses a job that needs more sheets than `limits.confirm_above_sheets` unless the request confirms it. In that case `platen_print_document` returns a non-error result with `"status": "confirmation_required"`. The result tells the agent to ask the person and to repeat the call with `confirm=true` only if they approve. Use `dry_run=true` to see page and sheet counts first.

Sources for `platen_print_document` (exactly one): `text`, `url` (fetched by Platen, subject to its fetch policy), `content_base64` (+ optional `filename`, `mime`), `scan_id`, `paperless_document_id`.
