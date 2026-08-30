//! One MCP endpoint over every deployed program.

use crate::Service;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, Implementation, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, RequestContext, RoleServer},
};
use std::sync::Arc;

/// Kept to a sentence deliberately. Every program carries its own usage, and a
/// server has one `instructions`; concatenating them would put a manual in
/// front of the model on every turn, which is what the usage field refuses to
/// be. They are published as resources instead.
const INSTRUCTIONS: &str = "Tools are named `{program}.{tool}`. Each program publishes a \
    `berm://{program}/usage` resource describing when to reach for its tools and how they go \
    together; read that before choosing among one program's tools.";

const USAGE_SCHEME: &str = "berm://";
const USAGE_PATH: &str = "/usage";

pub struct Mcp {
    service: Arc<Service>,
}

impl Mcp {
    pub fn new(service: Arc<Service>) -> Self {
        Self { service }
    }
}

impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tool_list_changed()
            .enable_resources()
            .enable_resources_list_changed()
            .build();

        // Named here rather than from rmcp's build environment, which would
        // report rmcp.
        ServerInfo::new(capabilities)
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }

    /// Deploying changes the tool set under clients already holding a list, so
    /// each session forwards that until its peer goes away.
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let mut changed = self.service.subscribe();
        let peer = context.peer.clone();
        tokio::spawn(async move {
            while changed.recv().await.is_ok() {
                if peer.notify_tool_list_changed().await.is_err() {
                    break;
                }
                let _ = peer.notify_resource_list_changed().await;
            }
        });
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .service
            .list()
            .iter()
            .flat_map(|deployed| {
                deployed.manifest().tools.iter().map(|tool| {
                    Tool::new(
                        format!("{}.{}", deployed.name, tool.name),
                        tool.description.clone(),
                        Arc::new(tool.parameters.as_object().cloned().unwrap_or_default()),
                    )
                })
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let Some((program, tool)) = request.name.split_once('.') else {
            return Err(McpError::invalid_params(
                format!(
                    "tool {:?} is not named `{{program}}.{{tool}}`",
                    request.name
                ),
                None,
            ));
        };

        // A program reads its arguments as the JSON its manifest advertised a
        // schema for, so the object goes across as it arrived.
        let args = match &request.arguments {
            Some(arguments) => serde_json::to_vec(arguments)
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?,
            None => Vec::new(),
        };

        match self.service.call(program, tool, args).await {
            Ok(Ok(result)) => Ok(CallToolResult::success(vec![ContentBlock::text(result)])),
            // The program ran and reported failure. That is a tool result the
            // model should see and react to, not a protocol error.
            Ok(Err(failure)) => Ok(CallToolResult::error(vec![ContentBlock::text(failure)])),
            Err(error) => Err(McpError::internal_error(format!("{error:#}"), None)),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = self
            .service
            .list()
            .iter()
            .filter(|deployed| !deployed.manifest().usage.is_empty())
            .map(|deployed| {
                Resource::new(
                    format!("{USAGE_SCHEME}{}{USAGE_PATH}", deployed.name),
                    format!("{} usage", deployed.name),
                )
                .with_description(format!(
                    "When to reach for {}'s tools, and how they go together.",
                    deployed.name
                ))
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let name = request
            .uri
            .strip_prefix(USAGE_SCHEME)
            .and_then(|rest| rest.strip_suffix(USAGE_PATH))
            .ok_or_else(|| McpError::resource_not_found(request.uri.clone(), None))?;

        let deployed = self
            .service
            .get(name)
            .ok_or_else(|| McpError::resource_not_found(request.uri.clone(), None))?;

        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            deployed.manifest().usage.clone(),
            request.uri,
        )]))
    }
}
