use serde::{Deserialize, Serialize};

pub(super) fn default_bark_server_url() -> String {
    "https://api.day.app".to_owned()
}

fn default_bark_content_mode() -> String {
    "body".to_owned()
}

fn default_bark_level() -> String {
    "active".to_owned()
}

fn default_bark_archive_mode() -> String {
    "inherit".to_owned()
}

fn default_bark_request_timeout_ms() -> u64 {
    4_000
}

fn default_bark_title_template() -> &'static str {
    "Nazh 通知 · {{payload.tag}}"
}

pub(super) fn default_bark_body_template() -> &'static str {
    "{{payload}}"
}

pub(super) fn normalize_bark_content_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "markdown" | "md" => "markdown",
        _ => "body",
    }
}

pub(super) fn normalize_bark_level(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" => "critical",
        "timesensitive" | "time_sensitive" | "time-sensitive" => "timeSensitive",
        "passive" => "passive",
        _ => "active",
    }
}

pub(super) fn normalize_bark_archive_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "archive" | "true" | "save" | "1" => "archive",
        "skip" | "false" | "0" | "no_archive" | "no-archive" => "skip",
        _ => "inherit",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarkPushNodeConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default = "default_bark_server_url")]
    pub server_url: String,
    pub device_key: String,
    #[serde(default = "default_bark_content_mode")]
    pub content_mode: String,
    #[serde(default)]
    pub title_template: String,
    #[serde(default)]
    pub subtitle_template: String,
    #[serde(default)]
    pub body_template: String,
    #[serde(default = "default_bark_level")]
    pub level: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub sound: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub copy: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub auto_copy: bool,
    #[serde(default)]
    pub call: bool,
    #[serde(default = "default_bark_archive_mode")]
    pub archive_mode: String,
    #[serde(default = "default_bark_request_timeout_ms")]
    pub request_timeout_ms: u64,
}

impl Default for BarkPushNodeConfig {
    fn default() -> Self {
        Self {
            connection_id: None,
            server_url: default_bark_server_url(),
            device_key: String::new(),
            content_mode: default_bark_content_mode(),
            title_template: default_bark_title_template().to_owned(),
            subtitle_template: String::new(),
            body_template: default_bark_body_template().to_owned(),
            level: default_bark_level(),
            badge: String::new(),
            sound: String::new(),
            icon: String::new(),
            group: String::new(),
            url: String::new(),
            copy: String::new(),
            image: String::new(),
            auto_copy: false,
            call: false,
            archive_mode: default_bark_archive_mode(),
            request_timeout_ms: default_bark_request_timeout_ms(),
        }
    }
}
