use mcp_multiplexer::config::*;

#[test]
fn parses_stdio_and_http() {
    let c: Config = serde_json::from_str(
        r#"{"mcpServers":{
        "a": {"command":"npx","args":["-y","srv"],"env":{"K":"V"}},
        "b": {"url":"https://x.test/mcp","headers":{"Authorization":"Bearer t"}}
    }}"#,
    )
    .unwrap();
    assert_eq!(c.mcp_servers["a"].command.as_deref(), Some("npx"));
    assert_eq!(
        c.mcp_servers["b"].url.as_deref(),
        Some("https://x.test/mcp")
    );
    c.validate().unwrap();
}

#[test]
fn rejects_neither_command_nor_url() {
    let c: Config = serde_json::from_str(r#"{"mcpServers":{"bad":{}}}"#).unwrap();
    let err = c.validate().unwrap_err().to_string();
    assert!(err.contains("bad"), "{err}");
}

#[test]
fn rejects_both_command_and_url() {
    let c: Config =
        serde_json::from_str(r#"{"mcpServers":{"bad":{"command":"x","url":"https://y"}}}"#)
            .unwrap();
    assert!(c.validate().is_err());
}

#[test]
fn rejects_duplicate_server_names() {
    let err = serde_json::from_str::<Config>(
        r#"{"mcpServers":{"a":{"command":"x"},"a":{"command":"y"}}}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("a"), "{err}");
}

#[test]
fn allow_deny_glob() {
    let c: Config = serde_json::from_str(
        r#"{"mcpServers":{"s":{
        "command":"x","allow":["get_*","ping"],"deny":["get_secret"]}}}"#,
    )
    .unwrap();
    let s = &c.mcp_servers["s"];
    assert!(s.is_allowed("get_user"));
    assert!(s.is_allowed("ping"));
    assert!(!s.is_allowed("post_user"));
    assert!(!s.is_allowed("get_secret")); // deny wins
    assert!(s.is_denied("get_secret"));
    assert!(!s.is_denied("get_user"));
}

#[test]
fn no_allow_means_all_allowed_except_deny() {
    let c: Config =
        serde_json::from_str(r#"{"mcpServers":{"s":{"command":"x","deny":["nuke"]}}}"#).unwrap();
    let s = &c.mcp_servers["s"];
    assert!(s.is_allowed("anything"));
    assert!(!s.is_allowed("nuke"));
}
