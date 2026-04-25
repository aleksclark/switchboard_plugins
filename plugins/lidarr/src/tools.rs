use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Artists ────────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_artists".into(),
            description: "List all artists in the Lidarr library. Returns artist details including names, status, quality, and path info.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "lidarr_get_artist".into(),
            description: "Get a specific artist by its Lidarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Lidarr artist ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "lidarr_add_artist".into(),
            description: "Add an artist to Lidarr. Requires a JSON body with artistName, foreignArtistId, qualityProfileId, metadataProfileId, rootFolderPath, and other artist properties.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of artist object (e.g. {"artistName":"...","foreignArtistId":"...","qualityProfileId":1,"metadataProfileId":1,"rootFolderPath":"/music","monitored":true,"addOptions":{"searchForMissingAlbums":true}})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "lidarr_update_artist".into(),
            description: "Update an existing artist in Lidarr. Provide the full artist object with modifications.".into(),
            parameters: HashMap::from([
                ("id".into(), "Lidarr artist ID (integer)".into()),
                ("body".into(), "JSON string of the full updated artist object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "lidarr_delete_artist".into(),
            description: "Delete an artist from Lidarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Lidarr artist ID (integer)".into()),
                ("delete_files".into(), "Also delete downloaded music files (true/false)".into()),
                ("add_import_exclusion".into(), "Add to import exclusion list to prevent re-adding (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "lidarr_lookup_artist".into(),
            description: "Search for artists to add to Lidarr by name or MusicBrainz ID. Use this before lidarr_add_artist to find the foreignArtistId and other required fields.".into(),
            parameters: HashMap::from([
                ("term".into(), "Search term — artist name or MusicBrainz ID".into()),
            ]),
            required: vec!["term".into()],
        },
        // ── Albums ─────────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_albums".into(),
            description: "List albums in the Lidarr library. Optionally filter by artist ID.".into(),
            parameters: HashMap::from([
                ("artist_id".into(), "Filter by Lidarr artist ID (integer)".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "lidarr_get_album".into(),
            description: "Get a specific album by its Lidarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Lidarr album ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "lidarr_add_album".into(),
            description: "Add an album to Lidarr. Requires a JSON body with album properties.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of album object (e.g. {"foreignAlbumId":"...","monitored":true,"addOptions":{"searchForNewAlbum":true}})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "lidarr_delete_album".into(),
            description: "Delete an album from Lidarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Lidarr album ID (integer)".into()),
                ("delete_files".into(), "Also delete downloaded album files (true/false)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Calendar ───────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_get_calendar".into(),
            description: "Get upcoming album releases within a date range".into(),
            parameters: HashMap::from([
                ("start".into(), "Start date in ISO 8601 format (e.g. 2024-01-01)".into()),
                ("end".into(), "End date in ISO 8601 format (e.g. 2024-12-31)".into()),
            ]),
            required: vec![],
        },
        // ── Queue ──────────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_get_queue".into(),
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
            name: "lidarr_delete_queue_item".into(),
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
            name: "lidarr_get_history".into(),
            description: "Get download and import history for artists and albums".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
                ("sort_key".into(), "Sort field (e.g. date)".into()),
                ("sort_direction".into(), "Sort direction: ascending or descending".into()),
                ("event_type".into(), "Filter by event type (e.g. grabbed, downloadImported, downloadFailed)".into()),
            ]),
            required: vec![],
        },
        // ── Commands ───────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_commands".into(),
            description: "List currently running or recently completed commands in Lidarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "lidarr_run_command".into(),
            description: r#"Execute a Lidarr command (e.g. RefreshArtist, ArtistSearch, RssSync, RenameFiles). Body should be a JSON command object."#.into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of command (e.g. {"name":"RefreshArtist"} or {"name":"ArtistSearch","artistId":1})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "lidarr_get_command".into(),
            description: "Get the status of a specific command by its ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Command ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── System ─────────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_get_status".into(),
            description: "Get Lidarr system status including version, OS, and runtime info".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Quality Profiles ───────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_quality_profiles".into(),
            description: "List all quality profiles. Use this to find qualityProfileId values needed when adding artists.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Metadata Profiles ──────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_metadata_profiles".into(),
            description: "List all metadata profiles. Use this to find metadataProfileId values needed when adding artists.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Root Folders ───────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_root_folders".into(),
            description: "List root folders configured in Lidarr. Use this to find rootFolderPath values needed when adding artists.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Tags ───────────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_list_tags".into(),
            description: "List all tags in Lidarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "lidarr_create_tag".into(),
            description: "Create a new tag in Lidarr".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of tag object (e.g. {"label":"my-tag"})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "lidarr_delete_tag".into(),
            description: "Delete a tag by ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Tag ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Blocklist ──────────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_get_blocklist".into(),
            description: "Get the blocklist of releases that Lidarr will not download".into(),
            parameters: HashMap::from([
                ("page".into(), "Page number (default 1)".into()),
                ("page_size".into(), "Page size (default 20)".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "lidarr_delete_blocklist_item".into(),
            description: "Delete an entry from the blocklist".into(),
            parameters: HashMap::from([
                ("id".into(), "Blocklist entry ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Wanted/Missing ─────────────────────────────────────────
        ToolDefinition {
            name: "lidarr_get_wanted_missing".into(),
            description: "Get a list of monitored albums that are missing (not yet downloaded)".into(),
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
