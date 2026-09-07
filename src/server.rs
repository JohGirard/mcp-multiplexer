use std::sync::Arc;
use rmcp::{RoleServer, ServerHandler, tool, tool_handler, tool_router};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use crate::config::Config;
use crate::model::ToolInfo;
use crate::search::search;
use crate::upstream::Upstreams;

#[derive(Serialize)]
pub struct ServerSummary {
    pub name: String,
    pub status: String,
    pub tool_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Serialize)]
pub struct ToolSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct ServerArg {
    pub server: String,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct SearchArgs {
    pub query: String,
    pub server: Option<String>,
    pub limit: Option<usize>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DescribeArgs {
    pub server: String,
    pub tool: String,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct CallArgs {
    pub server: String,
    pub tool: String,
    pub arguments: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Clone)]
pub struct Aggregator {
    cfg: Config,
    ups: Arc<Upstreams>,
    tool_router: ToolRouter<Self>,
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

impl Aggregator {
    pub fn new(cfg: Config, ups: Arc<Upstreams>) -> Aggregator {
        Aggregator { cfg, ups, tool_router: Self::tool_router() }
    }

    pub async fn list_servers(&self) -> anyhow::Result<Vec<ServerSummary>> {
        let mut out = Vec::new();
        for name in self.ups.server_names() {
            let tools = self.ups.tools(&name).await.unwrap_or_default();
            out.push(ServerSummary {
                status: self.ups.status(&name).to_string(),
                instructions: self.ups.instructions(&name),
                tool_count: tools.len(),
                name,
            });
        }
        Ok(out)
    }

    pub async fn list_tools(&self, server: String) -> anyhow::Result<Vec<ToolSummary>> {
        Ok(self.ups.tools(&server).await?.into_iter().map(|t| ToolSummary {
            name: t.name,
            description: t.description,
            annotations: t.annotations,
        }).collect())
    }

    pub async fn search_tools(&self, query: String, server: Option<String>, limit: usize)
        -> anyhow::Result<Vec<(String, ToolInfo)>> {
        let mut index = Vec::new();
        for name in self.ups.server_names() {
            index.push((name.clone(), self.ups.tools(&name).await.unwrap_or_default()));
        }
        Ok(search(&index, &query, server.as_deref(), limit))
    }

    pub async fn describe_tool(&self, server: String, tool: String) -> anyhow::Result<ToolInfo> {
        self.ups.tools(&server).await?.into_iter().find(|t| t.name == tool)
            .ok_or_else(|| anyhow::anyhow!("unknown tool {tool:?} on server {server:?}"))
    }

    pub async fn call_tool(&self, server: String, tool: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>)
        -> Result<CallToolResult, ErrorData> {
        self.ups.call(&server, &tool, arguments).await.map_err(internal)
    }

    /// Split a `server__tool` name into (server, tool) by longest matching
    /// expose-server prefix — server names may themselves contain `__`.
    pub fn route_exposed<'n>(&self, name: &'n str) -> Option<(&str, &'n str)> {
        self.cfg.mcp_servers.iter()
            .filter(|(_, sc)| sc.expose)
            .filter_map(|(n, _)| name.strip_prefix(&format!("{n}__")).map(|t| (n.as_str(), t)))
            .max_by_key(|(n, _)| n.len())
    }
}

#[tool_router]
impl Aggregator {
    #[tool(name = "list_servers", description = "List connected MCP servers: name, status, tool count, instructions")]
    async fn list_servers_tool(&self) -> Result<String, ErrorData> {
        let v = self.list_servers().await.map_err(internal)?;
        serde_json::to_string_pretty(&v).map_err(internal)
    }

    #[tool(name = "list_tools", description = "List a server's tools: name, one-line description, annotations. No schemas.")]
    async fn list_tools_tool(&self, Parameters(p): Parameters<ServerArg>) -> Result<String, ErrorData> {
        let v = self.list_tools(p.server).await.map_err(internal)?;
        serde_json::to_string_pretty(&v).map_err(internal)
    }

    #[tool(name = "search_tools", description = "Search tools across servers by name/description. Returns matches WITH full input schemas.")]
    async fn search_tools_tool(&self, Parameters(p): Parameters<SearchArgs>) -> Result<String, ErrorData> {
        let v = self.search_tools(p.query, p.server, p.limit.unwrap_or(5)).await.map_err(internal)?;
        serde_json::to_string_pretty(&v).map_err(internal)
    }

    #[tool(name = "describe_tool", description = "Get the full input schema of one exact tool")]
    async fn describe_tool_tool(&self, Parameters(p): Parameters<DescribeArgs>) -> Result<String, ErrorData> {
        let v = self.describe_tool(p.server, p.tool).await.map_err(internal)?;
        serde_json::to_string_pretty(&v).map_err(internal)
    }

    #[tool(name = "call_tool", description = "Call a tool on an upstream MCP server. Arguments must match its schema.")]
    async fn call_tool_tool(&self, Parameters(p): Parameters<CallArgs>) -> Result<CallToolResult, ErrorData> {
        self.call_tool(p.server, p.tool, p.arguments).await
    }
}

#[tool_handler]
impl ServerHandler for Aggregator {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Aggregated MCP servers. Use list_servers → list_tools/search_tools → describe_tool → call_tool. Tools named server__tool are directly exposed.")
    }

    async fn list_tools(&self, _req: Option<PaginatedRequestParams>, _ctx: RequestContext<RoleServer>)
        -> Result<ListToolsResult, ErrorData> {
        let mut tools = self.tool_router.list_all();
        for (name, sc) in &self.cfg.mcp_servers {
            if !sc.expose { continue; }
            if let Ok(list) = self.ups.tools(name).await {
                for t in list {
                    let schema = match t.schema {
                        serde_json::Value::Object(m) => Arc::new(m),
                        _ => Arc::new(serde_json::Map::new()),
                    };
                    let mut tool = Tool::new_with_raw(
                        format!("{name}__{}", t.name),
                        t.description.map(std::borrow::Cow::Owned),
                        schema,
                    );
                    tool.annotations = t.annotations.and_then(|v| serde_json::from_value(v).ok());
                    tools.push(tool);
                }
            }
        }
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(&self, req: CallToolRequestParams, ctx: RequestContext<RoleServer>)
        -> Result<CallToolResponse, ErrorData> {
        if let Some((server, tool)) = self.route_exposed(&req.name) {
            let tool = tool.to_string();
            return self.ups.call(server, &tool, req.arguments).await
                .map(CallToolResponse::from)
                .map_err(internal);
        }
        let tcc = ToolCallContext::new(self, req, ctx);
        self.tool_router.call(tcc).await
    }
}
