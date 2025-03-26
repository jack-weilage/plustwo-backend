use serde::Deserialize;

pub mod video;

#[derive(Debug, Deserialize)]
pub struct QueryResponse<T> {
    pub data: T,
    pub extensions: QueryExtensions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryExtensions {
    pub duration_milliseconds: usize,
    #[serde(rename = "requestID")]
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryConnection<T> {
    pub edges: Vec<QueryEdge<T>>,
    pub page_info: QueryPageInfo,
    #[serde(default)]
    pub total_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryEdge<T> {
    pub cursor: Option<String>,
    pub node: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPageInfo {
    pub has_next_page: bool,
}
