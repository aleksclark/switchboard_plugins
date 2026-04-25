use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Movies ──────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_list_movies".into(),
            description: "List all movies in the Radarr library. Returns movie details including titles, quality, status, and file info.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "radarr_get_movie".into(),
            description: "Get a specific movie by its Radarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Radarr movie ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "radarr_add_movie".into(),
            description: "Add a movie to Radarr. Requires a JSON body with title, tmdbId, qualityProfileId, rootFolderPath, and other movie properties.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of movie object (e.g. {"title":"...","tmdbId":123,"qualityProfileId":1,"rootFolderPath":"/movies","monitored":true,"addOptions":{"searchForMovie":true}})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "radarr_update_movie".into(),
            description: "Update an existing movie in Radarr. Provide the full movie object with modifications.".into(),
            parameters: HashMap::from([
                ("id".into(), "Radarr movie ID (integer)".into()),
                ("body".into(), "JSON string of the full updated movie object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "radarr_delete_movie".into(),
            description: "Delete a movie from Radarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Radarr movie ID (integer)".into()),
                ("delete_files".into(), "Also delete downloaded movie files (true/false)".into()),
                ("add_import_exclusion".into(), "Add to import exclusion list to prevent re-adding (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "radarr_lookup_movie".into(),
            description: "Search for movies to add to Radarr by title or TMDB/IMDB ID. Use this before radarr_add_movie to find the tmdbId and other required fields.".into(),
            parameters: HashMap::from([
                ("term".into(), "Search term — movie title, TMDB ID (tmdb:12345), or IMDB ID (imdb:tt1234567)".into()),
            ]),
            required: vec!["term".into()],
        },
        // ── Calendar ────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_get_calendar".into(),
            description: "Get upcoming movies (physical/digital releases) within a date range".into(),
            parameters: HashMap::from([
                ("start".into(), "Start date in ISO 8601 format (e.g. 2024-01-01)".into()),
                ("end".into(), "End date in ISO 8601 format (e.g. 2024-12-31)".into()),
            ]),
            required: vec![],
        },
        // ── Queue ───────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_get_queue".into(),
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
            name: "radarr_delete_queue_item".into(),
            description: "Remove an item from the download queue".into(),
            parameters: HashMap::from([
                ("id".into(), "Queue item ID (integer)".into()),
                ("remove_from_client".into(), "Also remove from download client (true/false)".into()),
                ("blocklist".into(), "Add release to blocklist (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── History ─────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_get_history".into(),
            description: "Get download and import history for movies".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. date)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
                ("event_type".into(), "Filter by event type (e.g. grabbed, downloadFolderImported, downloadFailed)".into()),
            ]),
            required: vec![],
        },
        // ── Commands ────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_list_commands".into(),
            description: "List currently running or recently completed commands in Radarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "radarr_run_command".into(),
            description: r#"Execute a Radarr command (e.g. RefreshMovie, MoviesSearch, RssSync, RenameFiles). Body should be a JSON command object."#.into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of command (e.g. {"name":"RefreshMovie"} or {"name":"MoviesSearch","movieIds":[1,2,3]})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "radarr_get_command".into(),
            description: "Get the status of a specific command by its ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Command ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── System ──────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_get_status".into(),
            description: "Get Radarr system status including version, OS, and runtime info".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Quality Profiles ────────────────────────────────────────
        ToolDefinition {
            name: "radarr_list_quality_profiles".into(),
            description: "List all quality profiles. Use this to find qualityProfileId values needed when adding movies.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Root Folders ────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_list_root_folders".into(),
            description: "List root folders configured in Radarr. Use this to find rootFolderPath values needed when adding movies.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Tags ────────────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_list_tags".into(),
            description: "List all tags in Radarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "radarr_create_tag".into(),
            description: "Create a new tag in Radarr".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of tag object (e.g. {"label":"my-tag"})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "radarr_delete_tag".into(),
            description: "Delete a tag by ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Tag ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Blocklist ───────────────────────────────────────────────
        ToolDefinition {
            name: "radarr_get_blocklist".into(),
            description: "Get the blocklist of releases that Radarr will not download".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "radarr_delete_blocklist_item".into(),
            description: "Delete an entry from the blocklist".into(),
            parameters: HashMap::from([
                ("id".into(), "Blocklist entry ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Wanted/Missing ──────────────────────────────────────────
        ToolDefinition {
            name: "radarr_get_wanted_missing".into(),
            description: "Get a list of monitored movies that are missing (not yet downloaded)".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. title, year)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
            ]),
            required: vec![],
        },
    ]
}
