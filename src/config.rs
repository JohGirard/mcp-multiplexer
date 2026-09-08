use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Config {
    #[serde(rename = "mcpServers", deserialize_with = "no_dup_keys")]
    pub mcp_servers: BTreeMap<String, ServerConfig>,
}

fn no_dup_keys<'de, D>(d: D) -> Result<BTreeMap<String, ServerConfig>, D::Error>
where
    D: Deserializer<'de>,
{
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
                    return Err(serde::de::Error::custom(format!(
                        "duplicate server name: {k}"
                    )));
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
    /// Enable OAuth 2.0 (PKCE) for this server — url servers only
    pub oauth: bool,
    /// Pre-registered OAuth client ID; dynamic registration is used when unset
    pub oauth_client_id: Option<String>,
    /// Scopes to request; server defaults when empty
    pub oauth_scopes: Vec<String>,
    /// Fixed port for the 127.0.0.1 callback listener, for providers that
    /// require an exact pre-registered redirect URI. Default: ephemeral port
    pub oauth_redirect_port: Option<u16>,
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
        if self.is_denied(tool) {
            return false;
        }
        match &self.allow {
            Some(pats) => pats.iter().any(|p| glob_match(p, tool)),
            None => true,
        }
    }
}

/// `${VAR}` expansion using `get` for lookups. Unclosed `${` or an unset
/// variable is an error — fail at startup, not with a broken upstream later.
fn expand(s: &str, get: impl Fn(&str) -> Option<String>) -> anyhow::Result<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        let Some(j) = rest[i + 2..].find('}') else {
            anyhow::bail!("unclosed \"${{\" in {s:?}")
        };
        let var = &rest[i + 2..i + 2 + j];
        let val = get(var)
            .ok_or_else(|| anyhow::anyhow!("env var {var:?} referenced in config is not set"))?;
        out.push_str(&rest[..i]);
        out.push_str(&val);
        rest = &rest[i + 2 + j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl Config {
    /// Returns the parsed config and the raw file text (for cache hashing).
    pub fn load(path: &Path) -> anyhow::Result<(Config, String)> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
        let mut cfg: Config = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("invalid config {}: {e}", path.display()))?;
        cfg.expand_env()?;
        cfg.validate()?;
        Ok((cfg, text))
    }

    /// Expand ${VAR} in command/args/env/url/headers of every server.
    fn expand_env(&mut self) -> anyhow::Result<()> {
        for (name, s) in &mut self.mcp_servers {
            let r = (|| {
                let get = |v: &str| std::env::var(v).ok();
                if let Some(c) = &mut s.command {
                    *c = expand(c, get)?;
                }
                for a in &mut s.args {
                    *a = expand(a, get)?;
                }
                for v in s.env.values_mut() {
                    *v = expand(v, get)?;
                }
                if let Some(u) = &mut s.url {
                    *u = expand(u, get)?;
                }
                for v in s.headers.values_mut() {
                    *v = expand(v, get)?;
                }
                Ok(())
            })();
            r.map_err(|e: anyhow::Error| anyhow::anyhow!("server {name:?}: {e}"))?;
        }
        Ok(())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for (name, s) in &self.mcp_servers {
            match (&s.command, &s.url) {
                (None, None) => {
                    anyhow::bail!("server {name:?}: needs either \"command\" or \"url\"")
                }
                (Some(_), Some(_)) => {
                    anyhow::bail!("server {name:?}: has both \"command\" and \"url\", pick one")
                }
                _ => {}
            }
            if s.command.is_some()
                && (s.oauth
                    || s.oauth_client_id.is_some()
                    || !s.oauth_scopes.is_empty()
                    || s.oauth_redirect_port.is_some())
            {
                anyhow::bail!("server {name:?}: oauth options require \"url\", not \"command\"");
            }
            if !s.oauth
                && (s.oauth_client_id.is_some()
                    || !s.oauth_scopes.is_empty()
                    || s.oauth_redirect_port.is_some())
            {
                anyhow::bail!("server {name:?}: oauth_* options require \"oauth\": true");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_vars() {
        let get = |v: &str| (v == "A").then(|| "x".to_string());
        assert_eq!(expand("a-${A}-b", get).unwrap(), "a-x-b");
        assert_eq!(expand("plain", get).unwrap(), "plain");
        assert_eq!(expand("${A}${A}", get).unwrap(), "xx");
        assert!(
            expand("${MISSING}", get)
                .unwrap_err()
                .to_string()
                .contains("MISSING")
        );
        assert!(expand("${unclosed", get).is_err());
    }
}
