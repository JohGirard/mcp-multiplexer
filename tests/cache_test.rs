use mcp_multiplexer::cache::*;

#[test]
fn roundtrip() {
    let dir = std::env::temp_dir().join(format!("mcpagg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut c = Cache::default();
    c.servers.insert("s".into(), vec![ToolInfo {
        name: "t".into(), description: Some("d".into()),
        schema: serde_json::json!({"type":"object"}), annotations: None,
    }]);
    c.instructions.insert("s".into(), "use wisely".into());
    c.save_to(&dir.join("index.json"), 42).unwrap();
    let loaded = Cache::load_from(&dir.join("index.json"), 42);
    assert_eq!(loaded.servers["s"][0].name, "t");
    assert_eq!(loaded.instructions["s"], "use wisely");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn wrong_hash_or_corrupt_is_empty() {
    let dir = std::env::temp_dir().join(format!("mcpagg-test2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("index.json");
    std::fs::write(&p, "not json").unwrap();
    assert!(Cache::load_from(&p, 1).servers.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}
