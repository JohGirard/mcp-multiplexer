use mcp_multiplexer::{cache::Cache, config::Config, server::Aggregator, upstream::Upstreams};
use std::sync::Arc;
use std::time::Duration;

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

    let hits = a
        .search_tools("echo message".into(), None, 5)
        .await
        .unwrap();
    assert_eq!(hits[0].1.name, "echo");
    assert!(hits[0].1.schema.is_object());

    let d = a.describe_tool("mock".into(), "add".into()).await.unwrap();
    assert_eq!(d.name, "add");

    let mut args = serde_json::Map::new();
    args.insert("message".into(), "yo".into());
    let r = a
        .call_tool("mock".into(), "echo".into(), Some(args))
        .await
        .unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: yo");
}

#[tokio::test]
async fn exposed_routing_longest_prefix() {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(
        r#"{{"mcpServers":{{"a":{{"command":{bin:?},"expose":true}},"a__b":{{"command":{bin:?},"expose":true}}}}}}"#
    );
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Arc::new(Upstreams::new(cfg.clone(), Cache::default(), 0));
    let a = Aggregator::new(cfg, ups);

    // server names may legally contain __; a__b__echo must route to server a__b (longest prefix), tool echo
    let (server, tool) = a.route_exposed("a__b__echo").unwrap();
    assert_eq!((server, tool), ("a__b", "echo"));
    let mut args = serde_json::Map::new();
    args.insert("message".into(), "hi".into());
    let r = a
        .call_tool(server.into(), tool.into(), Some(args))
        .await
        .unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: hi");

    assert_eq!(a.route_exposed("a__echo"), Some(("a", "echo")));
    assert_eq!(a.route_exposed("search_tools"), None);
}

#[tokio::test]
async fn meta_tools_stay_lazy_with_dead_servers() {
    // one working mock, one unspawnable server, one that hangs without speaking MCP (10s connect timeout)
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(
        r#"{{"mcpServers":{{
        "mock":{{"command":{bin:?}}},
        "dead":{{"command":"/nonexistent-binary-xyz"}},
        "hang":{{"command":"sleep","args":["30"]}}
    }}}}"#
    );
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Arc::new(Upstreams::new(cfg.clone(), Cache::default(), 0));
    let a = Aggregator::new(cfg, ups.clone());

    // list_servers must not connect to anything: cold servers report instantly
    let servers = tokio::time::timeout(Duration::from_secs(5), a.list_servers())
        .await
        .expect("list_servers blocked on cold servers")
        .unwrap();
    assert_eq!(servers.len(), 3);
    for s in &servers {
        assert_eq!(s.status, "cold", "{} should be cold", s.name);
        assert_eq!(s.tool_count, 0);
    }

    // search scoped to one server must not touch the others
    let hits = tokio::time::timeout(
        Duration::from_secs(5),
        a.search_tools("echo".into(), Some("mock".into()), 5),
    )
    .await
    .expect("scoped search blocked on cold servers")
    .unwrap();
    assert!(hits.iter().all(|(s, _)| s == "mock"));
    assert!(hits.iter().any(|(_, t)| t.name == "echo"));
    assert_eq!(ups.status("dead"), "cold");
    assert_eq!(ups.status("hang"), "cold");

    // unknown server filter errors and lists available servers
    let err = a
        .search_tools("echo".into(), Some("nope".into()), 5)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("unknown server") && err.contains("mock"),
        "{err}"
    );

    // after mock connected, list_servers reflects its cached index without connecting the rest
    let servers = a.list_servers().await.unwrap();
    let mock = servers.iter().find(|s| s.name == "mock").unwrap();
    assert_eq!(mock.status, "connected");
    assert!(mock.tool_count > 0);
    assert_eq!(
        servers.iter().find(|s| s.name == "hang").unwrap().status,
        "cold"
    );
}

#[tokio::test]
async fn describe_blocked_tool_reports_config_block() {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"mock":{{"command":{bin:?},"deny":["echo"]}}}}}}"#);
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Arc::new(Upstreams::new(cfg.clone(), Cache::default(), 0));
    let a = Aggregator::new(cfg, ups);

    let err = a
        .describe_tool("mock".into(), "echo".into())
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("blocked by config"), "{err}");
    // genuinely nonexistent tools still report as unknown
    let err = a
        .describe_tool("mock".into(), "nosuch".into())
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown tool"), "{err}");
}
