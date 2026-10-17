//! One MCP endpoint over every deployed program.

use crate::Service;
use berm_api::{DryRun, Meter, Output, host};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, Implementation, ListResourcesResult,
        ListToolsResult, Meta, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo, Tool,
        ToolAnnotations,
    },
    service::{NotificationContext, RequestContext, RoleServer},
};
use serde::Deserialize;
use std::sync::Arc;

/// A server has one `instructions`, paid on every turn; each program's README
/// is a resource the model reads when it needs it.
const INSTRUCTIONS: &str = "Tools are named `{program}.{tool}`. Each program with a README \
    publishes it as a `berm://{program}/readme` resource; read it before choosing among one \
    program's tools.";

const README_SCHEME: &str = "berm://";
const README_PATH: &str = "/readme";
/// The `_meta` key a listed tool carries its terms under.
const TERMS: &str = "terms";
/// The tool that runs any listed tool dry.
const DRY_RUN: &str = "dry_run";

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
                let terms = self.service.host.terms(&deployed.name).unwrap_or_default();
                deployed
                    .manifest()
                    .tools
                    .iter()
                    .filter(|tool| tool.anonymous)
                    .map(move |tool| {
                        let mut listed = Tool::new(
                            format!("{}.{}", deployed.name, tool.name),
                            tool.description.clone(),
                            Arc::new(tool.parameters.as_object().cloned().unwrap_or_default()),
                        );
                        if let Some(returns) = tool.returns.as_ref().and_then(|v| v.as_object()) {
                            listed = listed.with_raw_output_schema(Arc::new(returns.clone()));
                        }
                        if tool.query {
                            listed = listed.annotate(ToolAnnotations::new().read_only(true));
                        }
                        if let Some(held) = terms.tools.get(&tool.name) {
                            let mut meta = Meta::new();
                            meta.insert(TERMS.into(), serde_json::json!(held));
                            listed = listed.with_meta(meta);
                        }
                        listed
                    })
            })
            .chain([dry_run()])
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if request.name == DRY_RUN {
            return self.dry_run(request).await;
        }
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

        if !self
            .service
            .get(program)
            .is_some_and(|deployed| deployed.reached_anonymously(tool))
        {
            return Err(McpError::invalid_params(
                format!("{program} has no tool named {tool:?} that takes anonymous calls"),
                None,
            ));
        }

        // A tool listed with an output schema answers with structured content
        // as well, which MCP requires of it.
        let structured = self.service.get(program).is_some_and(|deployed| {
            deployed
                .manifest()
                .tools
                .iter()
                .any(|spec| spec.name == tool && spec.returns.is_some())
        });

        match self.service.call(program, tool, args).await {
            Ok(Ok(result)) if structured => match serde_json::from_str(&result) {
                Ok(value) => Ok(CallToolResult::structured(value)),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "{program}.{tool} declares a result schema and answered something that is not JSON: {error}"
                ))])),
            },
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
            .filter(|deployed| !deployed.manifest().readme.is_empty())
            .map(|deployed| {
                Resource::new(
                    format!("{README_SCHEME}{}{README_PATH}", deployed.name),
                    format!("{} README", deployed.name),
                )
                .with_description(format!("{}'s README.", deployed.name))
                .with_mime_type("text/markdown")
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
            .strip_prefix(README_SCHEME)
            .and_then(|rest| rest.strip_suffix(README_PATH))
            .ok_or_else(|| McpError::resource_not_found(request.uri.clone(), None))?;

        let deployed = self
            .service
            .get(name)
            .ok_or_else(|| McpError::resource_not_found(request.uri.clone(), None))?;

        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            deployed.manifest().readme.clone(),
            request.uri,
        )]))
    }
}

impl Mcp {
    /// Run the listed tool `tool` names with `args`, nothing it changes kept,
    /// and answer what it answered beside what it used.
    async fn dry_run(&self, request: CallToolRequestParams) -> Result<CallToolResult, McpError> {
        let Dried { tool, run } = serde_json::from_value(serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let Some((program, tool)) = tool.split_once('.') else {
            return Err(McpError::invalid_params(
                format!("tool {tool:?} is not named `{{program}}.{{tool}}`"),
                None,
            ));
        };
        if !self
            .service
            .get(program)
            .is_some_and(|deployed| deployed.reached_anonymously(tool))
        {
            return Err(McpError::invalid_params(
                format!("{program} has no tool named {tool:?} that takes anonymous calls"),
                None,
            ));
        }
        let (headers, output) = self
            .service
            .host
            .dry_run(program, None, tool, run.arguments(), run.dry)
            .await
            .map_err(|denied| McpError::internal_error(denied.to_string(), None))?;
        let meter: Option<Meter> = headers
            .get(host::METER)
            .and_then(|value| serde_json::from_slice(value.as_bytes()).ok());
        let failed = matches!(output, Output::Failed(_));
        let answer = serde_json::json!({ "output": output, "meter": meter });
        Ok(match failed {
            true => CallToolResult::structured_error(answer),
            false => CallToolResult::structured(answer),
        })
    }
}

/// What `dry_run` is called with.
#[derive(Deserialize)]
struct Dried {
    /// `{program}.{tool}`, as listed.
    tool: String,
    #[serde(flatten)]
    run: DryRun,
}

/// The listing of `dry_run`.
fn dry_run() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["tool"],
        "properties": {
            "tool": {"type": "string", "description": "A listed tool, `{program}.{tool}`."},
            "args": {"description": "The tool's arguments, as it takes them."},
            "answers": {
                "type": "array",
                "description": "What a request the tool sends is answered with instead of being sent, by method and URL.",
                "items": {
                    "type": "object",
                    "required": ["method", "url", "status"],
                    "properties": {
                        "method": {"type": "string"},
                        "url": {"type": "string"},
                        "status": {"type": "integer"},
                        "headers": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}},
                        "body": {"type": "string"}
                    }
                }
            },
            "offline": {"type": "boolean", "description": "Refuse a request no answer matches, rather than send it."}
        }
    });
    Tool::new(
        DRY_RUN,
        "Runs a listed tool and keeps nothing it changes: no state, logs or database writes, no scheduled calls, in any program it reaches. Its requests to other sites are sent unless `answers` answers them or `offline` is set. Answers what the tool answered and what the call used: instructions, rows read and written, memory, bytes proxied, bytes logged.",
        Arc::new(schema.as_object().cloned().unwrap_or_default()),
    )
}
