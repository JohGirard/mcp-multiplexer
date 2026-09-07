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

#[tokio::test]
async fn allow_filtered_tool_blocked() {
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"mock":{{"command":{bin:?},"allow":["add"]}}}}}}"#);
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Upstreams::new(cfg, Cache::default(), 0);
    // echo is not in the allow list: hidden from the index AND refused on the call path
    let err = ups.call("mock", "echo", None).await.unwrap_err().to_string();
    assert!(err.contains("blocked by config"), "{err}");
    let names: Vec<_> = ups.tools("mock").await.unwrap().iter().map(|t| t.name.clone()).collect();
    assert!(!names.contains(&"echo".to_string()));
}

#[tokio::test]
async fn dead_upstream_reconnects_and_retries() {
    // mock self-terminates ~50ms after answering each echo call (MOCK_DIE_AFTER_CALL)
    let bin = env!("CARGO_BIN_EXE_mcp-mock");
    let text = format!(r#"{{"mcpServers":{{"mock":{{"command":{bin:?},"env":{{"MOCK_DIE_AFTER_CALL":"1"}}}}}}}}"#);
    let cfg: Config = serde_json::from_str(&text).unwrap();
    let ups = Upstreams::new(cfg, Cache::default(), 0);

    let mut args = serde_json::Map::new();
    args.insert("message".into(), "one".into());
    let r = ups.call("mock", "echo", Some(args)).await.unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: one");

    // wait for the child to exit, so the next call hits a dead transport
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // the call must reset the cached client, reconnect, re-list and retry — and succeed
    let mut args = serde_json::Map::new();
    args.insert("message".into(), "two".into());
    let r = ups.call("mock", "echo", Some(args)).await.unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: two");
    assert_eq!(ups.status("mock"), "connected");
}

#[tokio::test]
async fn unspawnable_server_errors_without_wedging() {
    // `true` spawns but exits immediately, so every connect attempt fails
    let text = r#"{"mcpServers":{"dead":{"command":"true"}}}"#;
    let cfg: Config = serde_json::from_str(text).unwrap();
    let ups = Upstreams::new(cfg, Cache::default(), 0);
    let err = ups.call("dead", "x", None).await.unwrap_err().to_string();
    assert!(!err.is_empty());
    assert_eq!(ups.status("dead"), "unavailable");
    // a later attempt retries the connect instead of wedging on a cached failure
    let err = ups.call("dead", "x", None).await.unwrap_err().to_string();
    assert!(!err.is_empty());
    assert_eq!(ups.status("dead"), "unavailable");
}
