use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TwitchVideo {
    pub created_at: DateTime<Utc>,
    pub id: String,
    pub title: String,
    pub length_seconds: u64,
}
