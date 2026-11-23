//! Runtime configuration, read from environment variables.
//!
//! | Variable          | Meaning                                                      |
//! |-------------------|--------------------------------------------------------------|
//! | `LARK_APP_ID`     | App ID of a custom app created on the Lark developer console |
//! | `LARK_APP_SECRET` | App secret of that app                                       |
//! | `LARK_DOMAIN`     | `lark` (default, open.larksuite.com) or `feishu` (open.feishu.cn), or a full base URL |
//!
//! When the credentials are missing the client starts in offline demo mode.

pub const LARK_BASE: &str = "https://open.larksuite.com";
pub const FEISHU_BASE: &str = "https://open.feishu.cn";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub app_id: String,
    pub app_secret: String,
    pub base_url: String,
}

impl Config {
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let app_id = get("LARK_APP_ID").filter(|s| !s.trim().is_empty())?;
        let app_secret = get("LARK_APP_SECRET").filter(|s| !s.trim().is_empty())?;
        let base_url = resolve_base(get("LARK_DOMAIN").as_deref());
        Some(Self { app_id, app_secret, base_url })
    }
}

pub fn resolve_base(domain: Option<&str>) -> String {
    match domain.map(str::trim) {
        None | Some("") | Some("lark") | Some("larksuite") => LARK_BASE.into(),
        Some("feishu") => FEISHU_BASE.into(),
        Some(url) => url.trim_end_matches('/').to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn missing_credentials_means_demo() {
        let env: HashMap<&str, &str> = HashMap::from([("LARK_APP_ID", "cli_x")]);
        assert!(Config::from_lookup(|k| env.get(k).map(|s| s.to_string())).is_none());
    }

    #[test]
    fn domains() {
        assert_eq!(resolve_base(None), LARK_BASE);
        assert_eq!(resolve_base(Some("feishu")), FEISHU_BASE);
        assert_eq!(resolve_base(Some("http://127.0.0.1:9/")), "http://127.0.0.1:9");
        let env: HashMap<&str, &str> = HashMap::from([
            ("LARK_APP_ID", "cli_x"),
            ("LARK_APP_SECRET", "s"),
            ("LARK_DOMAIN", "feishu"),
        ]);
        let c = Config::from_lookup(|k| env.get(k).map(|s| s.to_string())).unwrap();
        assert_eq!(c.base_url, FEISHU_BASE);
    }
}
