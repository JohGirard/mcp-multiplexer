use crate::model::ToolInfo;

pub fn search(
    index: &[(String, Vec<ToolInfo>)],
    query: &str,
    server: Option<&str>,
    limit: usize,
) -> Vec<(String, ToolInfo)> {
    let tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();
    let mut scored: Vec<(u32, String, ToolInfo)> = index.iter()
        .filter(|(name, _)| server.map_or(true, |s| s == name))
        .flat_map(|(name, tools)| tools.iter().map(move |t| (name, t)))
        .filter_map(|(name, t)| {
            let n = t.name.to_lowercase();
            let d = t.description.clone().unwrap_or_default().to_lowercase();
            // ponytail: naive substring scoring — swap for a real fuzzy matcher if ranking disappoints
            let score: u32 = tokens.iter().map(|tok| {
                (if n.contains(tok.as_str()) { 2 } else { 0 })
              + (if d.contains(tok.as_str()) { 1 } else { 0 })
            }).sum();
            (score > 0).then(|| (score, name.clone(), t.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(limit);
    scored.into_iter().map(|(_, n, t)| (n, t)).collect()
}
