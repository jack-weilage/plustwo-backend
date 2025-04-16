use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::{QueryConnection, QueryResponse, video::TwitchVideo};
use uuid::Uuid;

/// This is a a client ID I just grabbed from some open source project. The default unauthorized
/// one now requires some authorization, but this one works.
const TWITCH_CLIENT_ID: &str = "kd1unb4b3q4t58fwlpcbzcbnm76a8fp";

pub mod shared;

pub struct TwitchGqlClient {
    client: reqwest::Client,
}
impl TwitchGqlClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert("Client-ID", TWITCH_CLIENT_ID.parse().unwrap());

                headers
            })
            .build()
            .unwrap();

        Self { client }
    }

    pub async fn get_videos_by_user_and_cursor(
        &self,
        login: &str,
        cursor: Option<String>,
    ) -> reqwest::Result<QueryConnection<TwitchVideo>> {
        let res: QueryResponse<UserQueryResponse<VideosByUserAndCursorUser>> = self
            .client
            .post("https://gql.twitch.tv/gql")
            .json(&GenericQuery {
                query: format!(
                    r#"
                query {{
                    user(login: "{login}") {{
                        videos(type: ARCHIVE, after: "{}") {{
                            pageInfo {{
                                hasNextPage
                            }}
                            totalCount
                            edges {{
                                cursor
                                node {{
                                    createdAt
                                    id
                                    title
                                    lengthSeconds
                                }}
                            }}
                        }}
                    }}
                }}
                "#,
                    cursor.clone().unwrap_or_default()
                ),
            })
            .send()
            .await?
            .json()
            .await?;

        Ok(res.data.user.videos)
    }

    pub async fn get_comments_by_video_and_cursor(
        &self,
        video_id: &str,
        cursor: Option<String>,
    ) -> reqwest::Result<QueryConnection<CommentsByVideoAndCursorComment>> {
        let res: QueryResponse<CommentsByVideoAndCursorQueryResponse> = self
            .client
            .post("https://gql.twitch.tv/gql")
            .json(&GenericQuery {
                query: format!(
                    r#"
                query {{
                    video(id: "{video_id}") {{
                        comments(after: "{}") {{
                            pageInfo {{
                                hasNextPage
                            }}
                            edges {{
                                cursor
                                node {{
                                    commenter {{
                                        displayName
                                        id
                                    }}
                                    createdAt
                                    id
                                    message {{
                                        fragments {{
                                            text
                                        }}
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}
                "#,
                    cursor.unwrap_or_default()
                ),
            })
            .send()
            .await?
            .json()
            .await?;

        Ok(res.data.video.comments)
    }
    pub async fn get_stream_by_user(&self, login: &str) -> reqwest::Result<UserAndStreamByLogin> {
        let res: QueryResponse<UserQueryResponse<UserAndStreamByLogin>> = self
            .client
            .post("https://gql.twitch.tv/gql")
            .json(&GenericQuery {
                query: format!(
                    r#"
                query {{
                    user(login: "{login}") {{
                        id
                        profileImageURL(width: 300)
                        broadcastSettings {{
                            title
                        }}
                        stream {{
                            id
                            archiveVideo {{
                                createdAt
                                id
                                title
                                lengthSeconds
                            }}
                        }}
                    }}
                }}
                "#,
                ),
            })
            .send()
            .await?
            .json()
            .await?;

        Ok(res.data.user)
    }
}

pub async fn collect_from_cursor<T, F>(mut f: F) -> reqwest::Result<Vec<T>>
where
    F: AsyncFnMut(Option<String>, usize, &Vec<T>) -> reqwest::Result<QueryConnection<T>>,
{
    let mut cursor = None;
    let mut complete_list = Vec::new();
    let mut total_count = 0;

    loop {
        let chunk = f(cursor.clone(), total_count, &complete_list).await?;
        total_count = chunk.total_count;

        for video in chunk.edges {
            cursor = video.cursor;
            complete_list.push(video.node);
        }

        if !chunk.page_info.has_next_page {
            break;
        }
    }

    Ok(complete_list)
}

#[derive(Debug, Serialize, Deserialize)]
struct GenericQuery {
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserQueryResponse<T> {
    user: T,
}

//
//
//
//
//

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideosByUserAndCursorUser {
    pub videos: QueryConnection<TwitchVideo>,
}

//

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsByVideoAndCursorQueryResponse {
    pub video: CommentsByVideoAndCursorVideo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsByVideoAndCursorVideo {
    pub comments: QueryConnection<CommentsByVideoAndCursorComment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsByVideoAndCursorComment {
    pub commenter: Option<CommentsByVideoAndCursorUser>,
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
    pub message: CommentsByVideoAndCursorMessage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsByVideoAndCursorUser {
    pub display_name: String,
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsByVideoAndCursorMessage {
    pub fragments: Vec<CommentsByVideoAndCursorFragment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsByVideoAndCursorFragment {
    pub text: String,
}

//

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAndStreamByLogin {
    pub id: String,
    #[serde(rename = "profileImageURL")]
    pub profile_image_url: String,
    pub broadcast_settings: UserAndStreamByLoginBroadcastSettings,
    pub stream: Option<UserAndStreamByLoginStream>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAndStreamByLoginBroadcastSettings {
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAndStreamByLoginStream {
    pub id: String,
    pub archive_video: TwitchVideo,
}
