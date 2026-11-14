use std::collections::HashMap;
use switchboard_guest_sdk::ToolDefinition;

fn tool(
    name: &str,
    description: &str,
    params: &[(&str, &str)],
    required: &[&str],
) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: description.into(),
        parameters: params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>(),
        required: required.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        tool(
            "platen_list_devices",
            "List the printers and scanners Platen can use, with their ids, names, state and which one is the default. Start here before printing or scanning.",
            &[],
            &[],
        ),
        tool(
            "platen_printer_status",
            "Get one printer's status: state, ink or toner levels, loaded paper, duplex and colour support.",
            &[("printer_id", "Printer id from platen_list_devices")],
            &["printer_id"],
        ),
        tool(
            "platen_printer_queue",
            "List the jobs currently queued on a printer (the printer's own queue, not Platen's history).",
            &[("printer_id", "Printer id from platen_list_devices")],
            &["printer_id"],
        ),
        tool(
            "platen_print_document",
            "[write] Print a document on a physical printer. PDF, images and plain text are supported. Use dry_run=true first to see pages and sheets. If the job needs more paper than the configured threshold, the result says confirmation_required: ask the person, and only then repeat the same call with confirm=true. Give exactly one source: text, url, content_base64, scan_id or paperless_document_id.",
            &[
                ("printer", "Printer id from platen_list_devices. Empty means the default printer"),
                ("text", "Plain text to print as it is"),
                ("url", "Public http(s) URL Platen fetches and prints"),
                ("content_base64", "Document bytes, base64-encoded (PDF, JPEG, PNG, GIF, TIFF, WebP, BMP or text)"),
                ("filename", "File name hint for content_base64, e.g. report.pdf"),
                ("mime", "MIME type hint for content_base64, e.g. application/pdf"),
                ("scan_id", "Print (copy) a Platen scan"),
                ("paperless_document_id", "Print a Paperless-ngx document by its numeric id"),
                ("title", "Job title. Defaults to the document's name"),
                ("copies", "Number of copies (default 1)"),
                ("duplex", "off (default), long-edge or short-edge"),
                ("color", "auto (default), color or monochrome"),
                ("quality", "draft, normal (default) or high"),
                ("media", "Paper size: a4, letter, legal, a5, 4x6, 5x7 or a PWG name. Defaults to the loaded paper"),
                ("pages", "Page ranges, e.g. 1-3,5 or 2-. Empty prints everything"),
                ("fill", "true to make a picture cover the whole sheet, cropping what does not fit"),
                ("confirm", "true only after the person approved a job that needs confirmation"),
                ("dry_run", "true to report pages and sheets without printing"),
            ],
            &[],
        ),
        tool(
            "platen_list_print_jobs",
            "List Platen's print history (most recent first) with state, printer, pages and sheets.",
            &[("limit", "Maximum jobs to return (default 50)")],
            &[],
        ),
        tool(
            "platen_cancel_print_job",
            "[write] Cancel a print job by its Platen job id.",
            &[("job_id", "Platen job id from platen_print_document or platen_list_print_jobs")],
            &["job_id"],
        ),
        tool(
            "platen_scan_document",
            "[write] Scan the page on the scanner glass (or the feeder) and store it as a new scan. Returns the scan id; add more flatbed pages with platen_scan_next_page.",
            &[
                ("scanner", "Scanner id from platen_list_devices. Empty means the default scanner"),
                ("source", "flatbed (default) or feeder"),
                ("duplex", "true to scan both sides with a duplex feeder"),
                ("color", "color (default) or gray"),
                ("resolution", "dpi, 75 to 1200 (default 300)"),
                ("paper", "a4 (default), letter, legal, a5, full or WIDTHxHEIGHT in mm"),
                ("title", "Name for the scan"),
            ],
            &[],
        ),
        tool(
            "platen_scan_next_page",
            "[write] Scan one more flatbed page into an existing scan. Ask the person to put the next page on the glass first.",
            &[("scan_id", "Scan id")],
            &["scan_id"],
        ),
        tool(
            "platen_get_scan",
            "Get a scan's details: title, pages and document state.",
            &[("scan_id", "Scan id")],
            &["scan_id"],
        ),
        tool(
            "platen_list_scans",
            "List stored scans, most recent first.",
            &[("limit", "Maximum scans to return (default 50)")],
            &[],
        ),
        tool(
            "platen_rename_scan",
            "[write] Rename a scan.",
            &[("scan_id", "Scan id"), ("title", "New title")],
            &["scan_id", "title"],
        ),
        tool(
            "platen_delete_scan",
            "[write] Delete a stored scan. A copy already filed in Paperless is not touched.",
            &[("scan_id", "Scan id")],
            &["scan_id"],
        ),
        tool(
            "platen_file_scan_in_paperless",
            "[write] Upload a scan to Paperless-ngx with a title and optional tags, correspondent, document type and date. Missing tags are created.",
            &[
                ("scan_id", "Scan id"),
                ("title", "Document title"),
                ("tags", "JSON array of tag names, e.g. [\"tax\",\"2026\"]"),
                ("correspondent", "Correspondent name"),
                ("document_type", "Document type name"),
                ("created", "The document's own date, YYYY-MM-DD"),
                ("wait", "true to wait (up to two minutes) until Paperless has imported it"),
            ],
            &["scan_id"],
        ),
        tool(
            "platen_search_paperless",
            "Search Paperless-ngx documents (for example to print one with platen_print_document paperless_document_id).",
            &[("query", "Full-text query"), ("limit", "Maximum results (default 20)")],
            &["query"],
        ),
    ]
}
