use mcp_multiplexer::{cache::Cache, config::Config, upstream::Upstreams};

fn test_config() -> Config {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"mock":{{"command":{bin:?}}}}}}}"#);
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn lazy_connect_and_call() {
    let ups = Upstreams::new(test_config(), Cache::default(), 0);
    assert_eq!(ups.status("mock"), "cold");
    let tools = ups.tools("mock").await.unwrap();
    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"echo") && names.contains(&"add"), "{names:?}");
    assert_eq!(ups.status("mock"), "connected");
    assert_eq!(ups.instructions("mock").as_deref(), Some("mock upstream for tests"));

    let mut args = serde_json::Map::new();
    args.insert("message".into(), "hi".into());
    let res = ups.call("mock", "echo", Some(args)).await.unwrap();
    let text = res.content[0].as_text().unwrap().text.clone();
    assert_eq!(text, "echo: hi");
}

#[tokio::test]
async fn unknown_tool_rerefreshes_and_errors() {
    let ups = Upstreams::new(test_config(), Cache::default(), 0);
    let err = ups.call("mock", "nonexistent", None).await.unwrap_err().to_string();
    assert!(!err.is_empty());
    // after the failed call the index must be populated (re-list happened)
    assert_eq!(ups.status("mock"), "connected");
}

#[tokio::test]
async fn denied_tool_blocked() {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"mock":{{"command":{bin:?},"deny":["echo"]}}}}}}"#);
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Upstreams::new(cfg, Cache::default(), 0);
    let err = ups.call("mock", "echo", None).await.unwrap_err().to_string();
    assert!(err.contains("blocked by config"), "{err}");
    // and it is filtered from the index
    let names: Vec<_> = ups.tools("mock").await.unwrap().iter().map(|t| t.name.clone()).collect();
    assert!(!names.contains(&"echo".to_string()));
}
