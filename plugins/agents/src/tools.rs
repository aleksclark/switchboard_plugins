use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // =====================================================================
        // Project management
        // =====================================================================
        ToolDefinition {
            name: "agents_project_list".into(),
            description: "List all registered projects in the ARP server. Returns an array of Project objects with name and repo fields.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_project_register".into(),
            description: "Register a new project with the ARP server. A project defines agent templates that can be spawned into workspaces.".into(),
            parameters: HashMap::from([
                ("name".into(), "Unique project name".into()),
                ("repo".into(), "Path to the git repository for this project".into()),
                ("agents".into(), "JSON string of agent template array. Each template has: name, command, port_env, and optional a2a object with name, description, and skills".into()),
            ]),
            required: vec!["name".into(), "repo".into()],
        },
        ToolDefinition {
            name: "agents_project_unregister".into(),
            description: "Unregister a project from the ARP server. Fails if the project has active workspaces.".into(),
            parameters: HashMap::from([
                ("name".into(), "Name of the project to unregister".into()),
            ]),
            required: vec!["name".into()],
        },

        // =====================================================================
        // Workspace management
        // =====================================================================
        ToolDefinition {
            name: "agents_workspace_create".into(),
            description: "Create a new workspace for a project. Optionally auto-spawn agents from templates. Returns a Workspace with status, agents, and directory path.".into(),
            parameters: HashMap::from([
                ("name".into(), "Unique workspace name".into()),
                ("project".into(), "Name of the project this workspace belongs to".into()),
                ("auto_agents".into(), "JSON array of agent template names to auto-spawn (e.g. [\"agent-a\", \"agent-b\"])".into()),
            ]),
            required: vec!["name".into(), "project".into()],
        },
        ToolDefinition {
            name: "agents_workspace_list".into(),
            description: "List workspaces. Optionally filter by project name or status (active/inactive).".into(),
            parameters: HashMap::from([
                ("project".into(), "Filter by project name".into()),
                ("status".into(), "Filter by status: active or inactive".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_workspace_get".into(),
            description: "Get full details of a workspace including its agents, directory path, and creation time.".into(),
            parameters: HashMap::from([
                ("name".into(), "Workspace name".into()),
            ]),
            required: vec!["name".into()],
        },
        ToolDefinition {
            name: "agents_workspace_destroy".into(),
            description: "Destroy a workspace. Stops all agents, removes workspace state, and optionally removes the git worktree directory.".into(),
            parameters: HashMap::from([
                ("name".into(), "Workspace name to destroy".into()),
                ("keep_worktree".into(), "If true, preserve the git worktree directory on disk (default: false)".into()),
            ]),
            required: vec!["name".into()],
        },

        // =====================================================================
        // Agent lifecycle
        // =====================================================================
        ToolDefinition {
            name: "agents_agent_spawn".into(),
            description: "Spawn a new agent instance from a template in a workspace. Returns AgentInstance with id, port, direct_url, and proxy_url. Optionally send an initial prompt, set a custom name, narrow scope, or set permission level.".into(),
            parameters: HashMap::from([
                ("workspace".into(), "Workspace name to spawn the agent in".into()),
                ("template".into(), "Agent template name from the project".into()),
                ("name".into(), "Custom instance name (defaults to template name)".into()),
                ("prompt".into(), "Initial prompt to send to the agent after it reaches ready status".into()),
                ("scope".into(), "JSON array of project names to scope the child token (must be subset of caller's scope)".into()),
                ("permission".into(), "Permission level for the child agent: admin, project, or session (cannot exceed caller's level)".into()),
            ]),
            required: vec!["workspace".into(), "template".into()],
        },
        ToolDefinition {
            name: "agents_agent_list".into(),
            description: "List agent instances. Optionally filter by workspace, status (starting/ready/busy/error/stopping/stopped), or template name.".into(),
            parameters: HashMap::from([
                ("workspace".into(), "Filter by workspace name".into()),
                ("status".into(), "Filter by agent status: starting, ready, busy, error, stopping, stopped".into()),
                ("template".into(), "Filter by template name".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_agent_status".into(),
            description: "Get full status of an agent instance including its A2A AgentCard with ARP lifecycle metadata.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_agent_stop".into(),
            description: "Stop a running agent. Cancels any active A2A tasks, sends SIGTERM, waits a grace period, then SIGKILL if needed.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to stop".into()),
                ("grace_period_ms".into(), "Milliseconds to wait after SIGTERM before SIGKILL (default: server-defined)".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_agent_restart".into(),
            description: "Restart an agent. Stops and re-spawns with the same template, name, workspace, and env. The proxy_url remains stable; direct_url may change.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to restart".into()),
            ]),
            required: vec!["agent_id".into()],
        },

        // =====================================================================
        // Agent messaging (A2A via ARP)
        // =====================================================================
        ToolDefinition {
            name: "agents_agent_message".into(),
            description: "Send a text message to an agent via A2A. By default blocks until the agent responds. Use context_id to continue an existing conversation.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to message".into()),
                ("message".into(), "Text message to send".into()),
                ("context_id".into(), "Context ID to continue an existing conversation".into()),
                ("blocking".into(), "If true (default), wait for agent response. If false, return immediately after sending.".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_agent_task".into(),
            description: "Send a message to an agent and get back an A2A Task object with id and status for tracking. If the agent returns a Message instead of a Task, ARP wraps it in a synthetic completed Task.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("message".into(), "Text message to send".into()),
                ("context_id".into(), "Context ID to continue an existing conversation".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_agent_task_status".into(),
            description: "Get the current status of an A2A task including its state, artifacts, and message history.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("task_id".into(), "A2A task ID to check".into()),
                ("history_length".into(), "Number of recent messages to include in the response".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },

        // =====================================================================
        // A2A proxy discovery
        // =====================================================================
        ToolDefinition {
            name: "agents_discover".into(),
            description: "Discover all A2A agents registered with the ARP server. Returns enriched AgentCards for all agents with ready or busy status, including proxy URLs and ARP metadata.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_agent_card".into(),
            description: "Get the enriched A2A AgentCard for a specific agent via the ARP proxy. Includes metadata.arp with agent_id, workspace, project, template, status, direct_url, and started_at.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID, agent name, or workspace/instance_name composite".into()),
            ]),
            required: vec!["agent_id".into()],
        },

        // =====================================================================
        // A2A proxy messaging
        // =====================================================================
        ToolDefinition {
            name: "agents_proxy_send_message".into(),
            description: "Send an A2A message to an agent through the ARP proxy. The proxy routes by agent_id, agent name, or workspace/name. Returns the A2A SendMessageResponse (Task or Message).".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID, name, or workspace/instance_name to route to".into()),
                ("message".into(), "Text message to send".into()),
                ("context_id".into(), "Optional context ID for multi-turn conversation".into()),
                ("message_id".into(), "Optional message ID (auto-generated if omitted)".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_proxy_get_task".into(),
            description: "Get an A2A task via the ARP proxy. Proxies a GetTask request to the agent.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID to query".into()),
                ("task_id".into(), "A2A task ID to retrieve".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },
        ToolDefinition {
            name: "agents_proxy_cancel_task".into(),
            description: "Cancel an A2A task via the ARP proxy. Proxies a CancelTask request to the agent.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID".into()),
                ("task_id".into(), "A2A task ID to cancel".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },

        // =====================================================================
        // A2A proxy routing
        // =====================================================================
        ToolDefinition {
            name: "agents_route_message".into(),
            description: "Route an A2A message to an agent by skill tags. The ARP proxy finds the best matching agent (preferring ready over busy) based on AgentSkill tags.".into(),
            parameters: HashMap::from([
                ("message".into(), "Text message to send".into()),
                ("tags".into(), "JSON array of skill tags to match against (e.g. [\"coding\", \"testing\"])".into()),
                ("context_id".into(), "Optional context ID for multi-turn conversation".into()),
                ("message_id".into(), "Optional message ID (auto-generated if omitted)".into()),
            ]),
            required: vec!["message".into(), "tags".into()],
        },
    ]
}
