use std::collections::HashMap;

use eyre::Result;
use plustwo_database::DatabaseClient;
use plustwo_twitch_gql::{
    CommentsByVideoAndCursorComment, TwitchGqlClient, UserAndStreamByLoginStream,
    collect_from_cursor,
};
use twitch_api::eventsub::{
    Transport,
    channel::ChannelChatMessageV1,
    stream::{StreamOfflineV1, StreamOnlineV1},
};

use crate::{kind_from_message, twitch::TwitchClient};

#[derive(Debug, Clone)]
pub struct WatchedBroadcaster {
    pub broadcaster: plustwo_database::entities::broadcasters::Model,
    pub current_broadcast: Option<UserAndStreamByLoginStream>,
    pub is_watching: bool,
}
impl WatchedBroadcaster {
    pub async fn watch(
        &mut self,
        api: &TwitchClient,
        session_id: &str,
        watcher_id: &str,
    ) -> Result<()> {
        if self.is_watching {
            return Ok(());
        }

        tracing::info!(
            name = "SubscriptionStart",
            broadcaster = self.broadcaster.display_name
        );

        let transport = Transport::websocket(session_id);

        api.subscribe(
            transport.clone(),
            StreamOnlineV1::broadcaster_user_id(self.broadcaster.id.to_string()),
        )
        .await?;

        api.subscribe(
            transport.clone(),
            StreamOfflineV1::broadcaster_user_id(self.broadcaster.id.to_string()),
        )
        .await?;

        api.subscribe(
            transport.clone(),
            ChannelChatMessageV1::new(self.broadcaster.id.to_string(), watcher_id),
        )
        .await?;

        self.is_watching = true;

        tracing::info!(
            name = "SubscriptionComplete",
            broadcaster = self.broadcaster.display_name
        );

        Ok(())
    }

    pub async fn catchup(&self, db: &DatabaseClient, gql: &TwitchGqlClient) -> Result<()> {
        let Some(stream) = &self.current_broadcast else {
            return Ok(());
        };

        db.start_broadcast(
            stream.archive_video.id.parse()?,
            self.broadcaster.id,
            stream.archive_video.title.clone(),
            stream.archive_video.created_at.naive_utc(),
        )
        .await?;

        let mut chatter_map = HashMap::new();
        let mut messages = Vec::new();

        let comments: Vec<CommentsByVideoAndCursorComment> =
            collect_from_cursor(async |cursor, _, comments| {
                tracing::debug!(
                    name = "CatchupProgress",
                    broadcaster = self.broadcaster.display_name,
                    comments = comments.len(),
                    cursor = cursor
                );

                gql.get_comments_by_video_and_cursor(&stream.archive_video.id, cursor)
                    .await
            })
            .await?;

        for comment in comments {
            // Some users don't show up. Maybe they've deleted their account or been
            // banned?
            let Some(user) = comment.commenter else {
                continue;
            };

            let Some(message_kind) = kind_from_message(&comment.message) else {
                continue;
            };

            let chatter = plustwo_database::entities::chatters::Model {
                id: user.id.parse()?,
                display_name: user.display_name,
            };

            chatter_map.insert(chatter.id, chatter.clone());
            messages.push(plustwo_database::entities::messages::Model {
                id: comment.id,
                broadcast_id: stream.archive_video.id.parse()?,
                chatter_id: chatter.id,
                sent_at: comment.created_at.naive_utc(),
                message_kind,
            });
        }

        tracing::info!(
            name = "CatchupComplete",
            broadcaster = self.broadcaster.display_name,
            chatters = chatter_map.len(),
            messages = messages.len(),
        );

        // Insert chatters and messages in a huge block to significantly increase performance.
        db.insert_many_chatters(chatter_map.values()).await?;
        db.insert_many_messages(&messages).await?;

        Ok(())
    }
}
