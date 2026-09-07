use mcp_multiplexer::model::ToolInfo;
use mcp_multiplexer::search::search;

fn t(name: &str, desc: &str) -> ToolInfo {
    ToolInfo { name: name.into(), description: Some(desc.into()),
        schema: serde_json::json!({}), annotations: None }
}

#[test]
fn ranks_name_hits_first_and_filters() {
    let index = vec![
        ("github".to_string(), vec![
            t("create_issue", "Open a new issue"),
            t("delete_repo", "Remove a repository about issues"),
        ]),
        ("slack".to_string(), vec![t("post_message", "Send a message")]),
    ];
    let hits = search(&index, "issue", None, 5);
    assert_eq!(hits[0].1.name, "create_issue");
    let hits = search(&index, "message", Some("slack"), 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, "slack");
    assert!(search(&index, "zzz", None, 5).is_empty());
    assert_eq!(search(&index, "issue", None, 1).len(), 1);
}
