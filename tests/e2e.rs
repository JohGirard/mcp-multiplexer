use rmcp::ClientServiceExt;
use rmcp::service::ClientLifecycleMode;

/// Client that negotiates protocol 2026-07-28 (rmcp's default client is LATEST
/// = 2025-11-25, and the legacy `initialize` handshake caps at the newest
/// legacy version) so the e2e covers the SEP-2549 cache-hints code path.
struct ModernClient;

impl rmcp::ClientHandler for ModernClient {
    fn get_info(&self) -> rmcp::model::ClientConfig {
        let mut cfg = rmcp::model::ClientConfig::default();
        cfg.protocol_version = rmcp::model::ProtocolVersion::V_2026_07_28;
        cfg
    }
}

#[tokio::test]
async fn end_to_end() {
    let mock = env!("CARGO_BIN_EXE_mcp-mock");
    let dir = std::env::temp_dir().join(format!("mcpmux-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg_path = dir.join("config.json");
    std::fs::write(
        &cfg_path,
        format!(
            r#"{{"mcpServers":{{
        "a": {{"command": {mock:?}}},
        "b": {{"command": {mock:?}, "expose": true, "deny": ["fail"]}}
    }}}}"#
        ),
    )
    .unwrap();

    let home = dir.join("home"); // isolate cache
    std::fs::create_dir_all(&home).unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_mcp-multiplexer"));
    cmd.args(["--config", cfg_path.to_str().unwrap(), "--verbose"])
        .env("XDG_CACHE_HOME", &home);
    let client = ModernClient
        .serve_with_lifecycle(
            rmcp::transport::child_process::TokioChildProcess::new(cmd).unwrap(),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();

    // SEP-2549: a client on protocol 2026-07-28 must receive ttlMs/cacheScope
    // on tools/list — Claude Code rejects the whole result otherwise.
    let negotiated = client.peer().peer_info().unwrap().protocol_version.clone();
    assert!(
        negotiated >= rmcp::model::ProtocolVersion::V_2026_07_28,
        "test client unexpectedly negotiated {negotiated}"
    );
    let result = client.list_tools(None).await.unwrap();
    assert_eq!(result.ttl_ms, Some(0), "missing ttlMs on {negotiated}");
    assert_eq!(
        result.cache_scope,
        Some(rmcp::model::CacheScope::Public),
        "missing cacheScope on {negotiated}"
    );
    for expected in [
        "list_servers",
        "list_tools",
        "search_tools",
        "describe_tool",
        "call_tool",
        "refresh_tools",
        "authorize_server",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing {expected} in {names:?}"
        );
    }
    assert!(
        names.iter().any(|n| n == "b__echo"),
        "expose namespacing failed: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "b__fail"),
        "deny leaked into expose: {names:?}"
    );

    let mut args = serde_json::Map::new();
    args.insert("server".into(), "a".into());
    args.insert("tool".into(), "add".into());
    args.insert(
        "arguments".into(),
        serde_json::json!({"left": 2, "right": 3}),
    );
    let r = client
        .call_tool(rmcp::model::CallToolRequestParams::new("call_tool").with_arguments(args))
        .await
        .unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "5");

    // direct exposed call
    let mut args = serde_json::Map::new();
    args.insert("message".into(), "direct".into());
    let r = client
        .call_tool(rmcp::model::CallToolRequestParams::new("b__echo").with_arguments(args))
        .await
        .unwrap();
    assert_eq!(r.content[0].as_text().unwrap().text, "echo: direct");

    // refresh_tools re-indexes all servers
    let r = client
        .call_tool(rmcp::model::CallToolRequestParams::new("refresh_tools"))
        .await
        .unwrap();
    let text = r.content[0].as_text().unwrap().text.clone();
    assert!(
        text.contains(r#""name": "a""#) && text.contains(r#""status": "ok""#),
        "{text}"
    );

    std::fs::remove_dir_all(&dir).ok();
}
