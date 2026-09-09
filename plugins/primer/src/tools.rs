use switchboard_guest_sdk::ToolDefinition;

fn tool(
    name: &str,
    description: &str,
    parameters: &[(&str, &str)],
    required: &[&str],
) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: description.into(),
        parameters: parameters
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect(),
        required: required.iter().map(|value| (*value).into()).collect(),
    }
}

fn list_parameters() -> Vec<(&'static str, &'static str)> {
    vec![
        ("limit", "Maximum records to return, up to 200"),
        ("offset", "Records to skip"),
        ("q", "Free-text search query"),
        ("sort", "Field to sort by"),
        ("dir", "Sort direction: asc or desc"),
        ("filter", "Exact-match filter as column:value"),
    ]
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    let list = list_parameters();
    vec![
        tool("primer_list_media_items", "List Primer TV catalog media and video records. Start here to discover TV content and content ingest state.", &list, &[]),
        tool("primer_get_media_item", "Get one Primer TV catalog media item by UUID. Use after primer_list_media_items.", &[("id", "Media item UUID")], &["id"]),
        tool("primer_create_media_item", "Create a curated Primer TV media item.", &[("body", "JSON media item create body")], &["body"]),
        tool("primer_update_media_item", "Update curator-controlled fields on a Primer TV media item.", &[("id", "Media item UUID"), ("body", "JSON patch body")], &["id", "body"]),
        tool("primer_ingest_media_item", "Update a Primer TV media item from automated content ingest without changing curator locks.", &[("id", "Media item UUID"), ("body", "JSON ingest patch body")], &["id", "body"]),
        tool("primer_delete_media_item", "Delete a Primer TV media item.", &[("id", "Media item UUID")], &["id"]),
        tool("primer_get_schedule_grid", "Get the Primer TV programme schedule grid for a date range.", &[("from", "First day as YYYY-MM-DD"), ("days", "Number of days, 1 to 31")], &[]),
        tool("primer_list_schedule_entries", "List Primer TV programme schedule entries and slots.", &list, &[]),
        tool("primer_create_schedule_entry", "Create a Primer TV programme schedule entry.", &[("body", "JSON schedule entry body")], &["body"]),
        tool("primer_update_schedule_entry", "Update a Primer TV programme schedule entry.", &[("id", "Schedule entry UUID"), ("body", "JSON patch body")], &["id", "body"]),
        tool("primer_delete_schedule_entry", "Delete a Primer TV programme schedule entry.", &[("id", "Schedule entry UUID")], &["id"]),
        tool("primer_list_tv_devices", "List paired Primer TV playback devices.", &list, &[]),
        tool("primer_list_content_manifest_entries", "List Primer content ingest manifest entries, acquisition attempts, and presence status.", &list, &[]),
        tool("primer_sync_content_manifest", "Bulk synchronize desired Primer content ingest manifest entries into Primer TV.", &[("body", "JSON object containing an items array of desired manifest entries")], &["body"]),
        tool("primer_record_content_attempt", "Record a Primer content ingest acquisition attempt for a manifest slug.", &[("slug", "Manifest entry slug"), ("error", "Optional acquisition error")], &["slug"]),
        tool("primer_mark_content_present", "Mark a Primer content ingest manifest entry as present.", &[("slug", "Manifest entry slug")], &["slug"]),
        tool("primer_browse_jellyfin", "Browse or search Jellyfin media visible to Primer TV content ingest.", &[("parent_id", "Optional Jellyfin parent ID"), ("q", "Search query"), ("limit", "Maximum records, up to 200"), ("start_index", "Records to skip")], &[]),
        tool("primer_sync_jellyfin", "Synchronize the Jellyfin catalog into Primer TV content records.", &[], &[]),
        tool("primer_get_rotation_suggestions", "List Primer TV content rotation suggestions for programming availability.", &[("limit", "Maximum suggestions, 1 to 50")], &[]),
        tool("primer_rotate_content", "Rotate Primer TV content availability windows.", &[("body", "JSON rotation request with mediaItemIds, days, expireOpen, and limit")], &["body"]),
        tool("primer_get_tv_metrics", "Get Primer TV playback and catalog metrics.", &[("days", "Reporting window, 1 to 365 days")], &[]),
        tool("primer_list_reports", "List Primer TV instructional-time reports sent to the Primer LMS.", &list, &[]),
        tool("primer_run_reports", "Run Primer TV instructional-time reporting now.", &[], &[]),
        tool("primer_list_students", "List students managed by Primer Tasks. Start here to discover students before creating schedules.", &list, &[]),
        tool("primer_list_tasks", "List Primer task templates and revisions with status and search filters. Start here for assignments and task management.", &[("q", "Free-text search query"), ("limit", "Maximum records"), ("offset", "Records to skip"), ("sort", "Field to sort by"), ("dir", "Sort direction"), ("status", "Task status"), ("view", "Use templates for latest revision per template")], &[]),
        tool("primer_create_task", "Create a draft Primer task template revision.", &[("body", "JSON TaskInput with title, instructions, and requirements")], &["body"]),
        tool("primer_revise_task", "Create a new revision of a Primer task template.", &[("id", "Task template or revision UUID"), ("body", "JSON TaskInput")], &["id", "body"]),
        tool("primer_publish_task", "Publish a Primer task revision for scheduling.", &[("id", "Task revision UUID")], &["id"]),
        tool("primer_retire_task", "Retire a Primer task template.", &[("id", "Task template UUID")], &["id"]),
        tool("primer_list_task_schedules", "List Primer Tasks assignment schedules for students.", &list, &[]),
        tool("primer_create_task_schedule", "Create a one-off or recurring Primer task schedule for a student.", &[("body", "JSON ScheduleInput")], &["body"]),
        tool("primer_update_task_schedule", "Update a Primer task schedule.", &[("id", "Schedule UUID"), ("body", "JSON ScheduleInput")], &["id", "body"]),
        tool("primer_delete_task_schedule", "Delete a Primer task schedule.", &[("id", "Schedule UUID")], &["id"]),
        tool("primer_list_occurrences", "List generated Primer task occurrences, attempts, and verification states.", &list, &[]),
        tool("primer_get_occurrence", "Get one Primer task occurrence and its requirements.", &[("id", "Occurrence UUID")], &["id"]),
        tool("primer_decide_occurrence", "Accept or reject a Primer task occurrence verification requirement.", &[("id", "Occurrence UUID"), ("body", "JSON decision with accepted, reason, and optional requirementId")], &["id", "body"]),
        tool("primer_retry_occurrence", "Retry a Primer task occurrence or requirement attempt.", &[("id", "Occurrence UUID"), ("requirement_id", "Optional requirement UUID"), ("attempt_id", "Optional attempt UUID")], &["id"]),
        tool("primer_skip_occurrence", "Skip a Primer task occurrence.", &[("id", "Occurrence UUID")], &["id"]),
        tool("primer_cancel_occurrence", "Cancel a Primer task occurrence.", &[("id", "Occurrence UUID")], &["id"]),
    ]
}
