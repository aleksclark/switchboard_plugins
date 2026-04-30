use switchboard_guest_sdk::ToolDefinition;
use std::collections::HashMap;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // =====================================================================
        // Project management — ProjectService gRPC (HTTP: /v1/projects)
        // =====================================================================
        ToolDefinition {
            name: "agents_project_list".into(),
            description: "List all registered projects. Returns an array of Project objects with name, repo, branch, and agent templates.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_project_register".into(),
            description: "Register a new project. A project defines a git repo and agent templates that can be spawned in workspaces.".into(),
            parameters: HashMap::from([
                ("name".into(), "Unique project name".into()),
                ("repo".into(), "Path or URL to the git repository".into()),
                ("branch".into(), "Default branch for new workspaces (default: main)".into()),
                ("agents".into(), "JSON array of AgentTemplate objects. Each has: name (required), command (required), port_env, health_check ({path, interval_ms, timeout_ms, retries}), env (map), capabilities (string array), a2a_card_config ({name, description, skills, input_modes, output_modes, streaming})".into()),
            ]),
            required: vec!["name".into(), "repo".into()],
        },
        ToolDefinition {
            name: "agents_project_unregister".into(),
            description: "Unregister a project. Fails with FAILED_PRECONDITION if active workspaces with running agents exist.".into(),
            parameters: HashMap::from([
                ("name".into(), "Project name to unregister".into()),
            ]),
            required: vec!["name".into()],
        },

        // =====================================================================
        // Workspace management — WorkspaceService gRPC (HTTP: /v1/workspaces)
        // =====================================================================
        ToolDefinition {
            name: "agents_workspace_create".into(),
            description: "Create a new workspace (git worktree) for a project. Optionally auto-spawn agents from templates. Returns a Workspace with status, agents, dir, and created_at.".into(),
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
            description: "List workspaces with their agents and status. Optionally filter by project or status.".into(),
            parameters: HashMap::from([
                ("project".into(), "Filter by project name".into()),
                ("status".into(), "Filter by status: WORKSPACE_STATUS_ACTIVE or WORKSPACE_STATUS_INACTIVE".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_workspace_get".into(),
            description: "Get detailed information about a workspace including agents, directory path, and creation time.".into(),
            parameters: HashMap::from([
                ("name".into(), "Workspace name".into()),
            ]),
            required: vec!["name".into()],
        },
        ToolDefinition {
            name: "agents_workspace_destroy".into(),
            description: "Destroy a workspace. Stops all agents, cancels working tasks, removes workspace state, and optionally removes the git worktree directory.".into(),
            parameters: HashMap::from([
                ("name".into(), "Workspace name to destroy".into()),
                ("keep_worktree".into(), "If true, preserve the git worktree directory on disk (default: false)".into()),
            ]),
            required: vec!["name".into()],
        },

        // =====================================================================
        // Agent lifecycle — AgentService gRPC (HTTP: /v1/agents)
        // =====================================================================
        ToolDefinition {
            name: "agents_agent_spawn".into(),
            description: "Spawn a new A2A agent in a workspace from a project template. Returns AgentInstance with id, port, direct_url, proxy_url, and status. Optionally send an initial prompt, set a custom name, inject env vars, narrow scope, or set permission level.".into(),
            parameters: HashMap::from([
                ("workspace".into(), "Workspace name to spawn the agent in".into()),
                ("template".into(), "Agent template name from the project".into()),
                ("name".into(), "Custom instance name (defaults to template name). Required when spawning multiple agents of the same template.".into()),
                ("env".into(), "JSON object of additional environment variables (e.g. {\"DEBUG\": \"true\"})".into()),
                ("prompt".into(), "Initial prompt to send after agent reaches AGENT_STATUS_READY".into()),
                ("scope".into(), "JSON Scope object to narrow child token: {\"global\": false, \"projects\": [\"proj-a\"]}. Must be subset of caller's scope.".into()),
                ("permission".into(), "Permission level for child agent: PERMISSION_SESSION, PERMISSION_PROJECT, or PERMISSION_ADMIN. Cannot exceed caller's level.".into()),
            ]),
            required: vec!["workspace".into(), "template".into()],
        },
        ToolDefinition {
            name: "agents_agent_list".into(),
            description: "List agent instances. Optionally filter by workspace, status (AGENT_STATUS_STARTING/READY/BUSY/ERROR/STOPPING/STOPPED), or template name.".into(),
            parameters: HashMap::from([
                ("workspace".into(), "Filter by workspace name".into()),
                ("status".into(), "Filter by agent status enum value".into()),
                ("template".into(), "Filter by template name".into()),
            ]),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_agent_status".into(),
            description: "Get detailed status of an agent instance including its resolved A2A AgentCard (enriched with metadata.arp).".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_agent_stop".into(),
            description: "Gracefully stop an agent. Cancels working A2A tasks, sends SIGTERM, waits grace period, then SIGKILL if needed. Returns AgentInstance with final status.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to stop".into()),
                ("grace_period_ms".into(), "Milliseconds to wait after SIGTERM before SIGKILL (default: 5000)".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_agent_restart".into(),
            description: "Restart an agent (stop + spawn with same config). The proxy_url remains stable; direct_url may change. Previous A2A context_id sessions are lost.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID to restart".into()),
            ]),
            required: vec!["agent_id".into()],
        },

        // =====================================================================
        // Agent messaging — AgentService gRPC (HTTP: /v1/agents/{id}/messages|tasks)
        // =====================================================================
        ToolDefinition {
            name: "agents_agent_message".into(),
            description: "Send a text message to an agent via A2A SendMessage, proxied through ARP. By default blocks until the agent responds. Returns either a Task or a Message.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("message".into(), "Text message to send (becomes a TextPart in the A2A Message)".into()),
                ("context_id".into(), "A2A context_id for multi-turn conversations (omit to start new)".into()),
                ("blocking".into(), "If true (default), wait for response. If false, return immediately after sending.".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_agent_task".into(),
            description: "Create a task on an agent via A2A SendMessage. Always returns a Task for async tracking (never a bare Message). If the agent returns a Message, ARP wraps it in a synthetic completed Task.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("message".into(), "Task description (becomes a TextPart)".into()),
                ("context_id".into(), "A2A context_id to continue a conversation".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_agent_task_status".into(),
            description: "Get the status of a running A2A task via GetTask. Returns the Task with status.state (SUBMITTED/WORKING/COMPLETED/FAILED/CANCELED), artifacts, and message history.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID".into()),
                ("task_id".into(), "A2A task ID to check".into()),
                ("history_length".into(), "Maximum number of recent messages to include (default: 10)".into()),
            ]),
            required: vec!["agent_id".into(), "task_id".into()],
        },

        // =====================================================================
        // Discovery — DiscoveryService gRPC (HTTP: /v1/discover)
        // =====================================================================
        ToolDefinition {
            name: "agents_discover".into(),
            description: "Discover agents across workspaces or the network. Returns enriched AgentCards with ARP metadata. Supports local (managed agents only) and network (probe URLs for AgentCards) scopes, plus capability filtering.".into(),
            parameters: HashMap::from([
                ("scope".into(), "Discovery scope: DISCOVERY_SCOPE_LOCAL (default) or DISCOVERY_SCOPE_NETWORK".into()),
                ("capability".into(), "Filter by AgentSkill tag (e.g. \"coding\", \"review\")".into()),
                ("urls".into(), "JSON array of base URLs to probe for /.well-known/agent-card.json (for NETWORK scope)".into()),
            ]),
            required: vec![],
        },

        // =====================================================================
        // A2A proxy — HTTP endpoints under /a2a/
        // =====================================================================
        ToolDefinition {
            name: "agents_proxy_list".into(),
            description: "List all A2A AgentCards for agents with READY or BUSY status via the ARP proxy registry. Returns enriched cards with metadata.arp fields.".into(),
            parameters: HashMap::new(),
            required: vec![],
        },
        ToolDefinition {
            name: "agents_agent_card".into(),
            description: "Get the enriched A2A AgentCard for an agent via the ARP proxy. The card includes metadata.arp (agent_id, workspace, project, template, status, direct_url, started_at) and supportedInterfaces[0].url pointing to the proxy.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent instance ID, agent name, or workspace/instance_name composite".into()),
            ]),
            required: vec!["agent_id".into()],
        },
        ToolDefinition {
            name: "agents_proxy_send_message".into(),
            description: "Send an A2A SendMessage to an agent through the ARP proxy. Routes by agent_id, agent name, or workspace/name. Returns the A2A SendMessageResponse (Task or Message).".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID, name, or workspace/instance_name to route to".into()),
                ("message".into(), "Text message to send".into()),
                ("context_id".into(), "A2A context_id for multi-turn conversation".into()),
                ("message_id".into(), "Message ID (auto-generated if omitted)".into()),
            ]),
            required: vec!["agent_id".into(), "message".into()],
        },
        ToolDefinition {
            name: "agents_proxy_get_task".into(),
            description: "Get an A2A task via the ARP proxy. Proxies a GetTask request to the agent.".into(),
            parameters: HashMap::from([
                ("agent_id".into(), "Agent ID".into()),
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
        ToolDefinition {
            name: "agents_route_message".into(),
            description: "Route an A2A message to an agent by skill tags via /a2a/route/message:send. The ARP proxy finds the best matching agent (preferring READY over BUSY) based on AgentSkill tags.".into(),
            parameters: HashMap::from([
                ("message".into(), "Text message to send".into()),
                ("tags".into(), "JSON array of skill tags to match against (e.g. [\"coding\", \"review\"])".into()),
                ("context_id".into(), "A2A context_id for multi-turn conversation".into()),
                ("message_id".into(), "Message ID (auto-generated if omitted)".into()),
            ]),
            required: vec!["message".into(), "tags".into()],
        },
    ]
}
