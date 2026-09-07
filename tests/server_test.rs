use mcp_multiplexer::{cache::Cache, config::Config, server::Aggregator, upstream::Upstreams};
use std::sync::Arc;

async fn agg(expose: bool) -> Aggregator {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"mock":{{"command":{bin:?},"expose":{expose}}}}}}}"#);
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Arc::new(Upstreams::new(cfg.clone(), Cache::default(), 0));
    Aggregator::new(cfg, ups)
}

#[tokio::test]
async fn meta_tools_flow() {
    let a = agg(false).await;
    let servers = a.list_servers().await.unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "mock");

    let tools = a.list_tools("mock".into()).await.unwrap();
    assert!(tools.iter().any(|t| t.name == "echo"));

    let hits = a.search_tools("echo message".into(), None, 5).await.unwrap();
    assert_eq!(hits[0].1.name, "echo");
    assert!(hits[0].1.schema.is_object());

    let d = a.describe_tool("mock".into(), "add".into()).await.unwrap();
    assert_eq!(d.name, "add");

    let mut args = serde_json::Map::new();
    args.insert("message".into(), "yo".into());
    let r = a.call_tool("mock".into(), "echo".into(), Some(args)).await.unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: yo");
}

#[tokio::test]
async fn exposed_routing_longest_prefix() {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"a":{{"command":{bin:?},"expose":true}},"a__b":{{"command":{bin:?},"expose":true}}}}}}"#);
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Arc::new(Upstreams::new(cfg.clone(), Cache::default(), 0));
    let a = Aggregator::new(cfg, ups);

    // server names may legally contain __; a__b__echo must route to server a__b (longest prefix), tool echo
    let (server, tool) = a.route_exposed("a__b__echo").unwrap();
    assert_eq!((server, tool), ("a__b", "echo"));
    let mut args = serde_json::Map::new();
    args.insert("message".into(), "hi".into());
    let r = a.call_tool(server.into(), tool.into(), Some(args)).await.unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: hi");

    assert_eq!(a.route_exposed("a__echo"), Some(("a", "echo")));
    assert_eq!(a.route_exposed("search_tools"), None);
}
