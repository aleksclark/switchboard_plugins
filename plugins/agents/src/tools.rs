use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // =====================================================================
        // ProjectService gRPC
        // =====================================================================
        ToolDefinition {
            name: "agents_project_list".into(),
            description: "List all registered projects via gRPC ProjectService.ListProjects. Returns Project objects with name, repo, branch, and agent templates.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_project_register".into(),
            description: "Register a new project via gRPC ProjectService.RegisterProject. A project defines a git repo and agent templates for spawning.".into(),
            parameters: HashMap::from([
                ("name".into(), "Unique project name".into()),
                ("repo".into(), "Path or URL to the git repository".into()),
                ("branch".into(), "Default branch for new workspaces (default: main)".into()),
                ("agents".into(), "JSON array of AgentTemplate objects. Each has: name (required), command (required), port_env, capabilities (string array), a2a_card_config ({name, description, skills, input_modes, output_modes, streaming})".into()),
            ]),
            required: vec!["name".into(), "repo".into()],
        },
        ToolDefinition {
            name: "agents_project_unregister".into(),
            description: "Unregister a project via gRPC ProjectService.UnregisterProject. Fails if active workspaces with running agents exist.".into(),
            parameters: HashMap::from([
                ("name".into(), "Project name to unregister".into()),
            ]),
            required: vec!["name".into()],
        },

        // =====================================================================
        // WorkspaceService gRPC
        // =====================================================================
        ToolDefinition {
            name: "agents_workspace_create".into(),
            description: "Create a new workspace via gRPC WorkspaceService.CreateWorkspace. Creates a git worktree and optionally auto-spawns agents.".into(),
            parameters: HashMap::from([
                ("name".into(), "Unique workspace name (used as worktree branch name)".into()),
                ("project".into(), "Project to create workspace from (must be registered)".into()),
                ("branch".into(), "Git branch override (defaults to project's default branch)".into()),
                ("auto_agents".into(), "JSON array of template names to auto-spawn (e.g. [\"crush\", \"reviewer\"])".into()),
            ]),
            required: vec!["name".into(), "project".into()],
        },
        ToolDefinition {
            name: "agents_workspace_list".into(),
            description: "List workspaces via gRPC WorkspaceService.ListWorkspaces. Optionally filter by project or status.".into(),
            parameters: HashMap::from([
                ("project".into(), "Filter by project name".into()),
                ("status".into(), "Filter by status: active or inactive".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_workspace_get".into(),
            description: "Get workspace details via gRPC WorkspaceService.GetWorkspace. Returns agents, directory path, and creation time.".into(),
            parameters: HashMap::from([
                ("name".into(), "Workspace name".into()),
            ]),
            required: vec!["name".into()],
        },
        ToolDefinition {
            name: "agents_workspace_destroy".into(),
            description: "Destroy a workspace via gRPC WorkspaceService.DestroyWorkspace. Stops all agents, cancels working tasks, and optionally removes the worktree.".into(),
            parameters: HashMap::from([
                ("name".into(), "Workspace name to destroy".into()),
                ("keep_worktree".into(), "If true, preserve the git worktree directory on disk (default: false)".into()),
            ]),
            required: vec!["name".into()],
        },

        // =====================================================================
        // AgentService gRPC — lifecycle
        // =====================================================================
        ToolDefinition {
            name: "agents_agent_spawn".into(),
            description: "Spawn a new A2A agent via gRPC AgentService.SpawnAgent. Returns AgentInstance with id, port, direct_url, proxy_url, and status.".into(),
            parameters: HashMap::from([
                ("workspace".into(), "Workspace name to spawn the agent in".into()),
                ("template".into(), "Agent template name from the project".into()),
                ("name".into(), "Custom instance name (defaults to template name)".into()),
                ("env".into(), "JSON object of additional environment variables".into()),
                ("prompt".into(), "Initial prompt to send after agent reaches READY".into()),
                ("scope".into(), "JSON Scope object: {\"global\": false, \"projects\": [\"proj-a\"]}".into()),
                ("permission".into(), "Permission level: session, project, or admin".into()),
            ]),
            required: vec!["workspace".into(), "template".into()],
        },
        ToolDefinition {
            name: "agents_agent_list".into(),
            description: "List agent instances via gRPC AgentService.ListAgents. Optionally filter by workspace, status, or template.".into(),
            parameters: HashMap::from([
                ("workspace".into(), "Filter by workspace name".into()),
                ("status".into(), "Filter by status: starting, ready, busy, error, stopping, stopped".into()),
                ("template".into(), "Filter by template name".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_agent_status".into(),
            description: "Get agent status via gRPC AgentService.GetAgentStatus. Returns AgentInstance with resolved A2A AgentCard.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_agent_stop".into(),
            description: "Stop an agent via gRPC AgentService.StopAgent. Cancels working A2A tasks, sends SIGTERM, waits grace period, then SIGKILL.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to stop".into()),
                ("grace_period_ms".into(), "Milliseconds to wait after SIGTERM before SIGKILL (default: 5000)".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_agent_restart".into(),
            description: "Restart an agent via gRPC AgentService.RestartAgent. Stop + spawn with same config. proxy_url stays stable.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to restart".into()),
            ]),
            required: vec!["agent_id".into()],
        },

        // =====================================================================
        // AgentService gRPC — messaging
        // =====================================================================
        ToolDefinition {
            name: "agents_agent_message".into(),
            description: "Send a message to an agent via gRPC AgentService.SendAgentMessage. Proxies an A2A SendMessage. Returns either a Task or Message.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("message".into(), "Text message to send".into()),
                ("context_id".into(), "A2A context_id for multi-turn conversations".into()),
                ("blocking".into(), "If true (default), wait for response. If false, return immediately.".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_agent_task".into(),
            description: "Create a task on an agent via gRPC AgentService.CreateAgentTask. Always returns a Task for async tracking.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("message".into(), "Task description".into()),
                ("context_id".into(), "A2A context_id to continue a conversation".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_agent_task_status".into(),
            description: "Get task status via gRPC AgentService.GetAgentTaskStatus. Returns Task with status, artifacts, and history.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("task_id".into(), "A2A task ID to check".into()),
                ("history_length".into(), "Maximum number of recent messages to include (default: 10)".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },

        // =====================================================================
        // DiscoveryService gRPC
        // =====================================================================
        ToolDefinition {
            name: "agents_discover".into(),
            description: "Discover agents via gRPC DiscoveryService.DiscoverAgents. Returns enriched AgentCards. Supports local/network scope and capability filtering.".into(),
            parameters: HashMap::from([
                ("scope".into(), "Discovery scope: local (default) or network".into()),
                ("capability".into(), "Filter by AgentSkill tag (e.g. \"coding\")".into()),
                ("urls".into(), "JSON array of base URLs to probe for AgentCards (network scope)".into()),
            ]),
            required: vec![],
        },

        // =====================================================================
        // A2A proxy — HTTP endpoints (served by ARP HTTP server, not gRPC)
        // =====================================================================
        ToolDefinition {
            name: "agents_proxy_list".into(),
            description: "List all A2A AgentCards via the ARP HTTP proxy at /a2a/agents. Returns cards for READY/BUSY agents with metadata.arp fields.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_agent_card".into(),
            description: "Get an enriched A2A AgentCard via the ARP HTTP proxy. Includes metadata.arp and supportedInterfaces pointing to the proxy.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID, name, or workspace/instance_name".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_proxy_send_message".into(),
            description: "Send an A2A message through the ARP HTTP proxy at /a2a/agents/{id}/message:send. Routes by agent ID, name, or workspace/name.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID, name, or workspace/instance_name".into()),
                ("message".into(), "Text message to send".into()),
                ("context_id".into(), "A2A context_id for multi-turn conversation".into()),
                ("message_id".into(), "Message ID (auto-generated if omitted)".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_proxy_get_task".into(),
            description: "Get an A2A task via the ARP HTTP proxy.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID".into()),
                ("task_id".into(), "A2A task ID".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },
        ToolDefinition {
            name: "agents_proxy_cancel_task".into(),
            description: "Cancel an A2A task via the ARP HTTP proxy.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID".into()),
                ("task_id".into(), "A2A task ID to cancel".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },
        ToolDefinition {
            name: "agents_route_message".into(),
            description: "Route an A2A message by skill tags via the ARP HTTP proxy at /a2a/route/message:send. Finds best matching agent (prefers READY over BUSY).".into(),
            parameters: HashMap::from([
                ("message".into(), "Text message to send".into()),
                ("tags".into(), "JSON array of skill tags to match (e.g. [\"coding\"])".into()),
                ("context_id".into(), "A2A context_id for multi-turn conversation".into()),
                ("message_id".into(), "Message ID (auto-generated if omitted)".into()),
            ]),
            required: vec!["message".into(), "tags".into()],
        },
    ]
}
