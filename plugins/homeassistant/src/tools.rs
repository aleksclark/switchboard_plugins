use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "homeassistant_list_states".into(),
            description: "List all smart home device and entity states in Home Assistant. Start here to discover devices. Returns sensors, lights, switches, and other IoT device states.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_get_state".into(),
            description: "Get the current state of a specific smart home entity (e.g. light, sensor, switch, thermostat)".into(),
            parameters: HashMap::from([("entity_id".into(), "Entity ID (e.g. light.living_room, sensor.temperature)".into())]),
            required: vec!["entity_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_set_state".into(),
            description: "Create or update the state of an entity".into(),
            parameters: HashMap::from([
                ("entity_id".into(), "Entity ID".into()),
                ("state".into(), "New state value".into()),
                ("attributes".into(), "JSON string of entity attributes".into()),
            ]),
            required: vec!["entity_id".into(), "state".into()],
        },
        ToolDefinition {
            name: "homeassistant_delete_state".into(),
            description: "Delete an entity from Home Assistant".into(),
            parameters: HashMap::from([("entity_id".into(), "Entity ID to delete".into())]),
            required: vec!["entity_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_list_services".into(),
            description: "List all available services grouped by domain".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_call_service".into(),
            description: "Call a Home Assistant service (e.g. turn on a light, lock a door)".into(),
            parameters: HashMap::from([
                ("domain".into(), "Service domain (e.g. light, switch, automation)".into()),
                ("service".into(), "Service name (e.g. turn_on, turn_off, toggle)".into()),
                ("service_data".into(), "JSON string of service data (e.g. entity_id, brightness)".into()),
                ("return_response".into(), "Return service response data (true/false)".into()),
            ]),
            required: vec!["domain".into(), "service".into()],
        },
        ToolDefinition {
            name: "homeassistant_list_events".into(),
            description: "List available home automation event types with listener counts in Home Assistant".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_fire_event".into(),
            description: "Fire a custom event in Home Assistant".into(),
            parameters: HashMap::from([
                ("event_type".into(), "Event type name".into()),
                ("event_data".into(), "JSON string of event data".into()),
            ]),
            required: vec!["event_type".into()],
        },
        ToolDefinition {
            name: "homeassistant_get_history".into(),
            description: "Get state change history for entities over a time period".into(),
            parameters: HashMap::from([
                ("entity_id".into(), "Comma-separated entity IDs to filter (required)".into()),
                ("start_time".into(), "Start time in ISO 8601 format (defaults to 1 day ago)".into()),
                ("end_time".into(), "End time in ISO 8601 format".into()),
                ("minimal_response".into(), "Only return last_changed and state for intermediate states (true/false)".into()),
                ("no_attributes".into(), "Skip returning attributes for faster response (true/false)".into()),
                ("significant_changes_only".into(), "Only return significant state changes (true/false)".into()),
            ]),
            required: vec!["entity_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_get_logbook".into(),
            description: "Get Home Assistant smart home logbook entries showing what home automation events happened and when".into(),
            parameters: HashMap::from([
                ("start_time".into(), "Start time in ISO 8601 format (defaults to 1 day ago)".into()),
                ("end_time".into(), "End time in ISO 8601 format".into()),
                ("entity_id".into(), "Filter by single entity ID".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_get_config".into(),
            description: "Get Home Assistant configuration (location, version, components, units)".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_check_config".into(),
            description: "Validate the Home Assistant configuration.yaml file".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_render_template".into(),
            description: "Render a Jinja2 template with Home Assistant context (access states, attributes, etc.)".into(),
            parameters: HashMap::from([("template".into(), r#"Jinja2 template string (e.g. '{{ states("sensor.temperature") }}')"#.into())]),
            required: vec!["template".into()],
        },
        ToolDefinition {
            name: "homeassistant_get_error_log".into(),
            description: "Get the Home Assistant smart home server error log".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_list_calendars".into(),
            description: "List all calendar entities".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "homeassistant_get_calendar_events".into(),
            description: "Get events from a specific calendar within a time range".into(),
            parameters: HashMap::from([
                ("entity_id".into(), "Calendar entity ID (e.g. calendar.personal)".into()),
                ("start".into(), "Start time in ISO 8601 format".into()),
                ("end".into(), "End time in ISO 8601 format".into()),
            ]),
            required: vec!["entity_id".into(), "start".into(), "end".into()],
        },
        ToolDefinition {
            name: "homeassistant_handle_intent".into(),
            description: "Handle a voice assistant intent (e.g. turn on lights via natural language)".into(),
            parameters: HashMap::from([
                ("name".into(), "Intent name (e.g. HassTurnOn)".into()),
                ("data".into(), "JSON string of intent data (e.g. entity, area)".into()),
            ]),
            required: vec!["name".into()],
        },
        ToolDefinition {
            name: "homeassistant_get_automation".into(),
            description: "Get the configuration of a specific automation by its ID".into(),
            parameters: HashMap::from([("automation_id".into(), "Automation unique ID".into())]),
            required: vec!["automation_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_save_automation".into(),
            description: "Create or update an automation".into(),
            parameters: HashMap::from([
                ("automation_id".into(), "Unique ID for the automation".into()),
                ("config".into(), r#"JSON string of automation config: {"alias": "...", "triggers": [...], "actions": [...]}"#.into()),
            ]),
            required: vec!["automation_id".into(), "config".into()],
        },
        ToolDefinition {
            name: "homeassistant_delete_automation".into(),
            description: "Delete a UI-managed automation by its ID".into(),
            parameters: HashMap::from([("automation_id".into(), "Automation unique ID to delete".into())]),
            required: vec!["automation_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_get_scene".into(),
            description: "Get the configuration of a specific scene by its ID".into(),
            parameters: HashMap::from([("scene_id".into(), "Scene unique ID".into())]),
            required: vec!["scene_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_save_scene".into(),
            description: "Create or update a scene".into(),
            parameters: HashMap::from([
                ("scene_id".into(), "Unique ID for the scene".into()),
                ("config".into(), r#"JSON string of scene config: {"name": "...", "entities": {...}}"#.into()),
            ]),
            required: vec!["scene_id".into(), "config".into()],
        },
        ToolDefinition {
            name: "homeassistant_delete_scene".into(),
            description: "Delete a UI-managed scene by its ID".into(),
            parameters: HashMap::from([("scene_id".into(), "Scene unique ID to delete".into())]),
            required: vec!["scene_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_get_script".into(),
            description: "Get the configuration of a specific script by its ID".into(),
            parameters: HashMap::from([("script_id".into(), "Script unique ID".into())]),
            required: vec!["script_id".into()],
        },
        ToolDefinition {
            name: "homeassistant_save_script".into(),
            description: "Create or update a script".into(),
            parameters: HashMap::from([
                ("script_id".into(), "Unique ID for the script".into()),
                ("config".into(), r#"JSON string of script config: {"alias": "...", "sequence": [...]}"#.into()),
            ]),
            required: vec!["script_id".into(), "config".into()],
        },
        ToolDefinition {
            name: "homeassistant_delete_script".into(),
            description: "Delete a UI-managed script by its ID".into(),
            parameters: HashMap::from([("script_id".into(), "Script unique ID to delete".into())]),
            required: vec!["script_id".into()],
        },
    ]
}
