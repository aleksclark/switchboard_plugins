use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Series ─────────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_list_series".into(),
            description: "List all TV series in the Sonarr library. Returns series details including titles, seasons, quality, status, and file info.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "sonarr_get_series".into(),
            description: "Get a specific TV series by its Sonarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Sonarr series ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "sonarr_add_series".into(),
            description: "Add a TV series to Sonarr. Requires a JSON body with title, tvdbId, qualityProfileId, rootFolderPath, and other series properties.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of series object (e.g. {"title":"...","tvdbId":123,"qualityProfileId":1,"rootFolderPath":"/tv","monitored":true,"addOptions":{"searchForMissingEpisodes":true}})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "sonarr_update_series".into(),
            description: "Update an existing TV series in Sonarr. Provide the full series object with modifications.".into(),
            parameters: HashMap::from([
                ("id".into(), "Sonarr series ID (integer)".into()),
                ("body".into(), "JSON string of the full updated series object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "sonarr_delete_series".into(),
            description: "Delete a TV series from Sonarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Sonarr series ID (integer)".into()),
                ("delete_files".into(), "Also delete downloaded episode files (true/false)".into()),
                ("add_import_exclusion".into(), "Add to import exclusion list to prevent re-adding (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "sonarr_lookup_series".into(),
            description: "Search for TV series to add to Sonarr by title or TVDB ID. Use this before sonarr_add_series to find the tvdbId and other required fields.".into(),
            parameters: HashMap::from([
                ("term".into(), "Search term — series title or TVDB ID (tvdb:12345)".into()),
            ]),
            required: vec!["term".into()],
        },
        // ── Episodes ───────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_get_episodes".into(),
            description: "Get all episodes for a specific TV series by its Sonarr series ID".into(),
            parameters: HashMap::from([
                ("series_id".into(), "Sonarr series ID (integer)".into()),
            ]),
            required: vec!["series_id".into()],
        },
        ToolDefinition {
            name: "sonarr_get_episode".into(),
            description: "Get a specific episode by its Sonarr episode ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Sonarr episode ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Calendar ───────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_get_calendar".into(),
            description: "Get upcoming episodes within a date range".into(),
            parameters: HashMap::from([
                ("start".into(), "Start date in ISO 8601 format (e.g. 2024-01-01)".into()),
                ("end".into(), "End date in ISO 8601 format (e.g. 2024-12-31)".into()),
                ("include_series".into(), "Include full series object with each episode (true/false)".into()),
            ]),
            required: vec![],
        },
        // ── Queue ──────────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_get_queue".into(),
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
            name: "sonarr_delete_queue_item".into(),
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
            name: "sonarr_get_history".into(),
            description: "Get download and import history for episodes".into(),
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
            name: "sonarr_list_commands".into(),
            description: "List currently running or recently completed commands in Sonarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "sonarr_run_command".into(),
            description: r#"Execute a Sonarr command (e.g. RefreshSeries, SeriesSearch, RssSync, RenameFiles). Body should be a JSON command object."#.into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of command (e.g. {"name":"RefreshSeries"} or {"name":"SeriesSearch","seriesId":1})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "sonarr_get_command".into(),
            description: "Get the status of a specific command by its ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Command ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── System ─────────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_get_status".into(),
            description: "Get Sonarr system status including version, OS, and runtime info".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Quality Profiles ───────────────────────────────────────
        ToolDefinition {
            name: "sonarr_list_quality_profiles".into(),
            description: "List all quality profiles. Use this to find qualityProfileId values needed when adding series.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Root Folders ───────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_list_root_folders".into(),
            description: "List root folders configured in Sonarr. Use this to find rootFolderPath values needed when adding series.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Tags ───────────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_list_tags".into(),
            description: "List all tags in Sonarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "sonarr_create_tag".into(),
            description: "Create a new tag in Sonarr".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of tag object (e.g. {"label":"my-tag"})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "sonarr_delete_tag".into(),
            description: "Delete a tag by ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Tag ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Blocklist ──────────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_get_blocklist".into(),
            description: "Get the blocklist of releases that Sonarr will not download".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "sonarr_delete_blocklist_item".into(),
            description: "Delete an entry from the blocklist".into(),
            parameters: HashMap::from([
                ("id".into(), "Blocklist entry ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Wanted/Missing ─────────────────────────────────────────
        ToolDefinition {
            name: "sonarr_get_wanted_missing".into(),
            description: "Get a list of monitored episodes that are missing (not yet downloaded)".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. airDateUtc, title)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
            ]),
            required: vec![],
        },
    ]
}
