use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Authors ────────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_authors".into(),
            description: "List all authors in the Readarr library. Returns author details including names, status, and book counts.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "readarr_get_author".into(),
            description: "Get a specific author by their Readarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Readarr author ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "readarr_add_author".into(),
            description: "Add an author to Readarr. Requires a JSON body with authorName, foreignAuthorId, qualityProfileId, metadataProfileId, rootFolderPath, and other author properties.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of author object (e.g. {"authorName":"...","foreignAuthorId":"...","qualityProfileId":1,"metadataProfileId":1,"rootFolderPath":"/books","monitored":true,"addOptions":{"monitor":"all","searchForMissingBooks":true}})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "readarr_update_author".into(),
            description: "Update an existing author in Readarr. Provide the full author object with modifications.".into(),
            parameters: HashMap::from([
                ("id".into(), "Readarr author ID (integer)".into()),
                ("body".into(), "JSON string of the full updated author object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "readarr_delete_author".into(),
            description: "Delete an author from Readarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Readarr author ID (integer)".into()),
                ("delete_files".into(), "Also delete downloaded book files (true/false)".into()),
                ("add_import_exclusion".into(), "Add to import exclusion list to prevent re-adding (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "readarr_lookup_author".into(),
            description: "Search for authors to add to Readarr by name. Use this before readarr_add_author to find the foreignAuthorId and other required fields.".into(),
            parameters: HashMap::from([
                ("term".into(), "Search term — author name".into()),
            ]),
            required: vec!["term".into()],
        },
        // ── Books ──────────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_books".into(),
            description: "List all books in the Readarr library. Optionally filter by author ID.".into(),
            parameters: HashMap::from([
                ("author_id".into(), "Filter by author ID (integer)".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "readarr_get_book".into(),
            description: "Get a specific book by its Readarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Readarr book ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "readarr_add_book".into(),
            description: "Add a book to Readarr. Requires a JSON body with title, foreignBookId, authorId, qualityProfileId, rootFolderPath, and other book properties.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of book object (e.g. {"title":"...","foreignBookId":"...","authorId":1,"qualityProfileId":1,"rootFolderPath":"/books","monitored":true,"addOptions":{"searchForNewBook":true}})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "readarr_update_book".into(),
            description: "Update an existing book in Readarr. Provide the full book object with modifications.".into(),
            parameters: HashMap::from([
                ("id".into(), "Readarr book ID (integer)".into()),
                ("body".into(), "JSON string of the full updated book object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "readarr_delete_book".into(),
            description: "Delete a book from Readarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Readarr book ID (integer)".into()),
                ("delete_files".into(), "Also delete downloaded book files (true/false)".into()),
                ("add_import_exclusion".into(), "Add to import exclusion list to prevent re-adding (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "readarr_lookup_book".into(),
            description: "Search for books to add to Readarr by title or ISBN. Use this before readarr_add_book to find the foreignBookId and other required fields.".into(),
            parameters: HashMap::from([
                ("term".into(), "Search term — book title or ISBN".into()),
            ]),
            required: vec!["term".into()],
        },
        // ── Calendar ───────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_get_calendar".into(),
            description: "Get upcoming book releases within a date range".into(),
            parameters: HashMap::from([
                ("start".into(), "Start date in ISO 8601 format (e.g. 2024-01-01)".into()),
                ("end".into(), "End date in ISO 8601 format (e.g. 2024-12-31)".into()),
            ]),
            required: vec![],
        },
        // ── Queue ──────────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_get_queue".into(),
            description: "Get the current download queue showing in-progress and pending downloads".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. timeleft, title)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "readarr_delete_queue_item".into(),
            description: "Remove an item from the download queue".into(),
            parameters: HashMap::from([
                ("id".into(), "Queue item ID (integer)".into()),
                ("remove_from_client".into(), "Also remove from download client (true/false)".into()),
                ("blocklist".into(), "Add release to blocklist (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── History ────────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_get_history".into(),
            description: "Get download and import history for books".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. date)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
                ("event_type".into(), "Filter by event type (e.g. grabbed, downloadFolderImported, downloadFailed)".into()),
            ]),
            required: vec![],
        },
        // ── Commands ───────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_commands".into(),
            description: "List currently running or recently completed commands in Readarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "readarr_run_command".into(),
            description: r#"Execute a Readarr command (e.g. RefreshAuthor, AuthorSearch, BookSearch, RssSync). Body should be a JSON command object."#.into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of command (e.g. {"name":"RefreshAuthor"} or {"name":"BookSearch","bookIds":[1,2,3]})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "readarr_get_command".into(),
            description: "Get the status of a specific command by its ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Command ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── System ─────────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_get_status".into(),
            description: "Get Readarr system status including version, OS, and runtime info".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Quality Profiles ───────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_quality_profiles".into(),
            description: "List all quality profiles. Use this to find qualityProfileId values needed when adding authors or books.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Metadata Profiles ──────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_metadata_profiles".into(),
            description: "List all metadata profiles. Use this to find metadataProfileId values needed when adding authors.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Root Folders ───────────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_root_folders".into(),
            description: "List root folders configured in Readarr. Use this to find rootFolderPath values needed when adding authors or books.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Tags ───────────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_list_tags".into(),
            description: "List all tags in Readarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "readarr_create_tag".into(),
            description: "Create a new tag in Readarr".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of tag object (e.g. {"label":"my-tag"})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "readarr_delete_tag".into(),
            description: "Delete a tag by ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Tag ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Blocklist ──────────────────────────────────────────────
        ToolDefinition {
            name: "readarr_get_blocklist".into(),
            description: "Get the blocklist of releases that Readarr will not download".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "readarr_delete_blocklist_item".into(),
            description: "Delete an entry from the blocklist".into(),
            parameters: HashMap::from([
                ("id".into(), "Blocklist entry ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Wanted/Missing ─────────────────────────────────────────
        ToolDefinition {
            name: "readarr_get_wanted_missing".into(),
            description: "Get a list of monitored books that are missing (not yet downloaded)".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. title, releaseDate)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
            ]),
            required: vec![],
        },
    ]
}
