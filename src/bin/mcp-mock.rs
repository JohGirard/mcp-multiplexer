use rmcp::model::*;
use rmcp::{ServerHandler, ServiceExt, tool, tool_router};

#[derive(Clone)]
struct Mock;

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct EchoParams {
    message: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct AddParams {
    left: usize,
    right: usize,
}

#[tool_router]
impl Mock {
    #[tool(description = "Echo back the message")]
    async fn echo(
        &self,
        rmcp::handler::server::wrapper::Parameters(p): rmcp::handler::server::wrapper::Parameters<
            EchoParams,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        die_after_call();
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "echo: {}",
            p.message
        ))]))
    }
    #[tool(description = "Add two numbers")]
    async fn add(
        &self,
        rmcp::handler::server::wrapper::Parameters(p): rmcp::handler::server::wrapper::Parameters<
            AddParams,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(
            (p.left + p.right).to_string(),
        )]))
    }
    #[tool(description = "Always fails")]
    async fn fail(&self) -> Result<CallToolResult, ErrorData> {
        Err(ErrorData::internal_error("mock failure", None))
    }
}

/// Test hook: with MOCK_DIE_AFTER_CALL set, exit shortly after answering a call,
/// simulating an upstream that dies mid-session (delay lets the response flush first).
fn die_after_call() {
    if std::env::var_os("MOCK_DIE_AFTER_CALL").is_some() {
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            std::process::exit(0);
        });
    }
}

#[rmcp::tool_handler]
impl ServerHandler for Mock {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_instructions("mock upstream for tests")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let svc = Mock.serve(rmcp::transport::stdio()).await?;
    svc.waiting().await?;
    Ok(())
}
