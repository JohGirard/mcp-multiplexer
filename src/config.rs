use std::collections::BTreeMap;
use std::path::Path;
use serde::{Deserialize, Deserializer};
use schemars::JsonSchema;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Config {
    #[serde(rename = "mcpServers", deserialize_with = "no_dup_keys")]
    pub mcp_servers: BTreeMap<String, ServerConfig>,
}

fn no_dup_keys<'de, D>(d: D) -> Result<BTreeMap<String, ServerConfig>, D::Error>
where D: Deserializer<'de> {
    use serde::de::{MapAccess, Visitor};
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = BTreeMap<String, ServerConfig>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a map of server name to server config")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut out = BTreeMap::new();
            while let Some((k, v)) = map.next_entry::<String, ServerConfig>()? {
                if out.insert(k.clone(), v).is_some() {
                    return Err(serde::de::Error::custom(format!("duplicate server name: {k}")));
                }
            }
            Ok(out)
        }
    }
    d.deserialize_map(V)
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    /// Bypass the meta-tools: expose this server's tools directly as server__tool
    pub expose: bool,
    /// If set, only these tools are visible (exact names or prefix* globs)
    pub allow: Option<Vec<String>>,
    /// Always hidden/blocked; wins over allow
    pub deny: Vec<String>,
}

pub fn glob_match(pat: &str, name: &str) -> bool {
    match pat.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pat == name,
    }
}

impl ServerConfig {
    pub fn is_denied(&self, tool: &str) -> bool {
        self.deny.iter().any(|p| glob_match(p, tool))
    }
    pub fn is_allowed(&self, tool: &str) -> bool {
        if self.is_denied(tool) { return false; }
        match &self.allow {
            Some(pats) => pats.iter().any(|p| glob_match(p, tool)),
            None => true,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Config> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
        let cfg: Config = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("invalid config {}: {e}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for (name, s) in &self.mcp_servers {
            match (&s.command, &s.url) {
                (None, None) => anyhow::bail!("server {name:?}: needs either \"command\" or \"url\""),
                (Some(_), Some(_)) => anyhow::bail!("server {name:?}: has both \"command\" and \"url\", pick one"),
                _ => {}
            }
        }
        Ok(())
    }
}
