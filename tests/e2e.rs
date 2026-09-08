use rmcp::ServiceExt;

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
    let client =
        ().serve(rmcp::transport::child_process::TokioChildProcess::new(cmd).unwrap())
            .await
            .unwrap();

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
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
