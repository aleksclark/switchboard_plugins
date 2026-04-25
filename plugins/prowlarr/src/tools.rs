use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Indexers ────────────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_list_indexers".into(),
            description: "List all configured indexers in Prowlarr. Returns indexer details including name, protocol, privacy, status, and capabilities.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "prowlarr_get_indexer".into(),
            description: "Get a specific indexer by its Prowlarr database ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Prowlarr indexer ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "prowlarr_add_indexer".into(),
            description: "Add a new indexer to Prowlarr. Use prowlarr_get_indexer_schema first to get the required fields and configuration for the indexer type.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of indexer object with name, implementation, configContract, fields, etc."#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "prowlarr_update_indexer".into(),
            description: "Update an existing indexer in Prowlarr. Provide the full indexer object with modifications.".into(),
            parameters: HashMap::from([
                ("id".into(), "Prowlarr indexer ID (integer)".into()),
                ("body".into(), "JSON string of the full updated indexer object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "prowlarr_delete_indexer".into(),
            description: "Delete an indexer from Prowlarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Prowlarr indexer ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "prowlarr_test_indexer".into(),
            description: "Test an indexer configuration to verify it is working. Provide the full indexer object to test.".into(),
            parameters: HashMap::from([
                ("body".into(), "JSON string of indexer object to test".into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "prowlarr_test_all_indexers".into(),
            description: "Test all configured indexers at once to verify they are all working".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "prowlarr_get_indexer_schema".into(),
            description: "Get the schema for all supported indexer types. Returns available indexer implementations and their required fields/configuration.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Search ─────────────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_search".into(),
            description: "Search across all configured indexers (or specific ones) for releases matching a query".into(),
            parameters: HashMap::from([
                ("query".into(), "Search query string".into()),
                ("indexer_ids".into(), "Comma-separated indexer IDs to search (e.g. \"1,2,3\"). Omit to search all.".into()),
                ("categories".into(), "Comma-separated category IDs to filter results (e.g. \"2000,5000\")".into()),
                ("type".into(), "Search type: search, tvsearch, moviesearch, musicsearch, or booksearch".into()),
            ]),
            required: vec!["query".into()],
        },
        ToolDefinition {
            name: "prowlarr_grab_release".into(),
            description: "Grab (download) a specific release from search results. Requires the guid and indexerId from a search result.".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string with release info (e.g. {"guid":"...","indexerId":1})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        // ── Applications ───────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_list_applications".into(),
            description: "List all synced application instances (Sonarr, Radarr, Lidarr, Readarr, etc.) configured in Prowlarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "prowlarr_get_application".into(),
            description: "Get a specific synced application by its ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Application ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "prowlarr_add_application".into(),
            description: "Add a new application sync to Prowlarr (e.g. sync indexers to Sonarr/Radarr). Use prowlarr_get_application_schema first to see required fields.".into(),
            parameters: HashMap::from([
                ("body".into(), "JSON string of application object with name, implementation, configContract, fields, syncLevel, etc.".into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "prowlarr_update_application".into(),
            description: "Update an existing application sync in Prowlarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Application ID (integer)".into()),
                ("body".into(), "JSON string of the full updated application object".into()),
            ]),
            required: vec!["id".into(), "body".into()],
        },
        ToolDefinition {
            name: "prowlarr_delete_application".into(),
            description: "Delete an application sync from Prowlarr".into(),
            parameters: HashMap::from([
                ("id".into(), "Application ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        ToolDefinition {
            name: "prowlarr_test_all_applications".into(),
            description: "Test all configured application syncs at once to verify connectivity".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "prowlarr_get_application_schema".into(),
            description: "Get the schema for all supported application types. Returns available implementations (Sonarr, Radarr, Lidarr, Readarr, etc.) and their required fields.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Indexer Proxies ────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_list_indexer_proxies".into(),
            description: "List all configured indexer proxies (FlareSolverr, HTTP proxies, etc.)".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── System ─────────────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_get_status".into(),
            description: "Get Prowlarr system status including version, OS, and runtime info".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        // ── Tags ───────────────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_list_tags".into(),
            description: "List all tags in Prowlarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "prowlarr_create_tag".into(),
            description: "Create a new tag in Prowlarr".into(),
            parameters: HashMap::from([
                ("body".into(), r#"JSON string of tag object (e.g. {"label":"my-tag"})"#.into()),
            ]),
            required: vec!["body".into()],
        },
        ToolDefinition {
            name: "prowlarr_delete_tag".into(),
            description: "Delete a tag by ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Tag ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Download Clients ───────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_list_download_clients".into(),
            description: "List all configured download clients in Prowlarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "prowlarr_get_download_client".into(),
            description: "Get a specific download client by its ID".into(),
            parameters: HashMap::from([
                ("id".into(), "Download client ID (integer)".into()),
            ]),
            required: vec!["id".into()],
        },
        // ── Notifications ──────────────────────────────────────────
        ToolDefinition {
            name: "prowlarr_list_notifications".into(),
            description: "List all configured notification connections in Prowlarr".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
    ]
}
