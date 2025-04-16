use std::collections::HashMap;

use eyre::{Context as _, Result, bail};
use plustwo_database::{
    DatabaseClient, DateTime,
    entities::{self, sea_orm_active_enums::MessageKind},
};
use plustwo_twitch_gql::{
    CommentsByVideoAndCursorComment, CommentsByVideoAndCursorMessage, TwitchGqlClient,
    UserAndStreamByLogin, UserAndStreamByLoginStream, collect_from_cursor,
};
use socket::EventSubSocket;
use twitch::TwitchClient;
use twitch_api::{
    TWITCH_EVENTSUB_WEBSOCKET_URL,
    eventsub::{
        Event as TwitchEvent, EventsubWebsocketData, Message, Payload, SessionData,
        channel::{ChannelChatMessageV1, ChannelChatMessageV1Payload},
        stream::{StreamOfflineV1, StreamOnlineV1, StreamOnlineV1Payload},
    },
    types::Timestamp,
};

mod socket;
mod twitch;

macro_rules! env_var {
    ($name:expr) => {
        ::std::env::var($name)
            .wrap_err_with(|| format!("Failed to find environment variable {}", $name))?
            .as_str()
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let graphql_client = TwitchGqlClient::new();
    let db = DatabaseClient::new(env_var!("DATABASE_URL")).await?;

    let mut api_client = TwitchClient::new(
        env_var!("TWITCH_CLIENT_SECRET"),
        env_var!("TWITCH_REFRESH_TOKEN"),
        env_var!("TWITCH_CLIENT_ID"),
    )
    .await?;
    api_client.refresh_token().await?;

    let mut current_broadcasts: HashMap<i64, i64> = HashMap::new();

    let mut eventsub = EventSubSocket::connect(TWITCH_EVENTSUB_WEBSOCKET_URL.as_str()).await?;
    loop {
        let msg = eventsub.next_message().await?;
        let event = twitch_api::eventsub::Event::parse_websocket(&msg)?;

        match event {
            // Sent when no event has occurred within the last 10s.
            EventsubWebsocketData::Keepalive { .. } => Ok(()),

            // Sent when the socket is first connected, must subscribe within 10s.
            EventsubWebsocketData::Welcome {
                payload: twitch_api::eventsub::WelcomePayload { session },
                ..
            } => {
                tracing::info!(name: "RecvWelcome", session = ?session);
                on_welcome(
                    &db,
                    &graphql_client,
                    &api_client,
                    &session,
                    &mut current_broadcasts,
                )
                .await
            }

            // Sent if the server that the client is connected to needs to swap.
            EventsubWebsocketData::Reconnect {
                payload: twitch_api::eventsub::ReconnectPayload { session },
                ..
            } => {
                tracing::warn!(name: "RecvReconnect", session = ?session);
                eventsub = EventSubSocket::connect(&session.reconnect_url.map_or_else(
                    || TWITCH_EVENTSUB_WEBSOCKET_URL.to_string(),
                    |url| url.to_string(),
                ))
                .await?;

                Ok(())
            }

            // Sent if Twitch revokes a subscription for any reason.
            EventsubWebsocketData::Revocation { metadata, .. } => bail!(
                "Subscription for channel \"{}\" revoked!",
                metadata.subscription_type
            ),

            // Sent when an event occurs.
            EventsubWebsocketData::Notification { metadata, payload } => match payload {
                TwitchEvent::StreamOnlineV1(Payload {
                    message: Message::Notification(payload),
                    ..
                }) => {
                    tracing::info!("StreamOnlineV1({})", payload.broadcaster_user_login);
                    on_stream_online(&graphql_client, &db, &payload, &mut current_broadcasts).await
                }
                TwitchEvent::StreamOfflineV1(Payload {
                    message: Message::Notification(payload),
                    ..
                }) => {
                    let user_id = payload.broadcaster_user_id.as_str().parse()?;
                    tracing::info!("StreamOfflineV1({})", payload.broadcaster_user_login);

                    db.end_broadcast(
                        user_id,
                        timestamp_to_time(&metadata.message_timestamp.into_owned())?,
                        current_broadcasts.get(&user_id).copied(),
                    )
                    .await?;

                    current_broadcasts.remove(&user_id);

                    Ok(())
                }
                TwitchEvent::ChannelChatMessageV1(Payload {
                    message: Message::Notification(payload),
                    ..
                }) => {
                    let Some(&broadcast_id) =
                        current_broadcasts.get(&payload.broadcaster_user_id.as_str().parse()?)
                    else {
                        continue;
                    };

                    on_chat_message(
                        &db,
                        &payload,
                        metadata.message_timestamp.into(),
                        broadcast_id,
                    )
                    .await
                }

                ev => {
                    tracing::warn!("Recieved unexpected notification: {ev:?}");
                    Ok(())
                }
            },

            ev => {
                tracing::warn!("Recieved unexpected message: {ev:?}");
                Ok(())
            }
        }?;
    }
}

async fn on_welcome(
    db: &DatabaseClient,
    gql: &TwitchGqlClient,
    api: &TwitchClient,
    session: &SessionData<'_>,
    current_broadcasts: &mut HashMap<i64, i64>,
) -> Result<()> {
    let watcher = gql.get_stream_by_user(env_var!("TWITCH_USER")).await?;

    for broadcaster_name in env_var!("TWITCH_BROADCASTERS").split(',') {
        let broadcaster = gql.get_stream_by_user(broadcaster_name).await?;

        handle_catchup(db, gql, &broadcaster, broadcaster_name, current_broadcasts).await?;

        let transport = twitch_api::eventsub::Transport::websocket(&session.id);

        api.subscribe(
            transport.clone(),
            StreamOnlineV1::broadcaster_user_id(broadcaster.id.as_str()),
        )
        .await?;

        api.subscribe(
            transport.clone(),
            StreamOfflineV1::broadcaster_user_id(broadcaster.id.as_str()),
        )
        .await?;

        api.subscribe(
            transport.clone(),
            ChannelChatMessageV1::new(broadcaster.id.as_str(), watcher.id.as_str()),
        )
        .await?;

        tracing::info!("Completed setup of {broadcaster_name}");
    }

    Ok(())
}

async fn on_stream_online(
    graphql_client: &TwitchGqlClient,
    db: &DatabaseClient,
    payload: &StreamOnlineV1Payload,
    current_broadcasts: &mut HashMap<i64, i64>,
) -> Result<()> {
    let broadcaster = graphql_client
        .get_stream_by_user(payload.broadcaster_user_login.as_str())
        .await?;

    let Some(stream) = broadcaster.stream else {
        bail!("Failed to find broadcast after StreamOnline for {broadcaster:?}")
    };

    let broadcaster_id = broadcaster.id.as_str().parse()?;
    let broadcast_id = stream.archive_video.id.parse()?;

    current_broadcasts.insert(broadcaster_id, broadcast_id);

    db.start_broadcast(
        broadcast_id,
        broadcaster_id,
        broadcaster.broadcast_settings.title,
        timestamp_to_time(&payload.started_at)?,
    )
    .await?;

    Ok(())
}

async fn on_chat_message(
    db: &DatabaseClient,
    payload: &ChannelChatMessageV1Payload,
    sent_at: twitch_api::types::Timestamp,
    broadcast_id: i64,
) -> Result<()> {
    let message_kind = match &payload.message.text {
        t if t.starts_with("+2") || t.ends_with("+2") => MessageKind::PlusTwo,
        t if t.starts_with("-2") || t.ends_with("-2") => MessageKind::MinusTwo,
        _ => return Ok(()),
    };

    tracing::info!(
        name: "ChatMessage",
        broadcaster = payload.broadcaster_user_name.as_str(),
        chatter = payload.chatter_user_name.as_str(),
        kind = ?message_kind
    );

    // Attempt to insert chatter if they don't exist.
    db.insert_chatter(
        payload.chatter_user_id.as_str().parse()?,
        payload.chatter_user_name.to_string(),
    )
    .await?;

    db.insert_message(
        payload.message_id.as_str().parse()?,
        broadcast_id,
        payload.chatter_user_id.as_str().parse()?,
        timestamp_to_time(&sent_at)?,
        message_kind,
    )
    .await?;

    Ok(())
}

async fn handle_catchup(
    db: &DatabaseClient,
    gql: &TwitchGqlClient,
    broadcaster: &UserAndStreamByLogin,
    broadcaster_name: &str,
    current_broadcasts: &mut HashMap<i64, i64>,
) -> Result<()> {
    let Some(stream) = &broadcaster.stream else {
        return Ok(());
    };

    tracing::info!(name: "CatchupStart", broadcaster = broadcaster_name);

    // Make sure that the current broadcast is tracked even if the watcher started in the
    // middle of it.
    current_broadcasts.insert(broadcaster.id.parse()?, stream.archive_video.id.parse()?);

    db.start_broadcast(
        stream.archive_video.id.parse()?,
        broadcaster.id.parse()?,
        stream.archive_video.title.clone(),
        stream.archive_video.created_at.naive_utc(),
    )
    .await?;

    let mut chatter_map = HashMap::new();
    let mut messages = Vec::new();

    let comments: Vec<CommentsByVideoAndCursorComment> =
        collect_from_cursor(async |cursor, _, comments| {
            tracing::info!(
                name: "CatchupProgress",
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

        let chatter = entities::chatters::Model {
            id: user.id.parse()?,
            display_name: user.display_name,
        };

        chatter_map.insert(chatter.id, chatter.clone());
        messages.push(entities::messages::Model {
            id: comment.id,
            broadcast_id: stream.archive_video.id.parse()?,
            chatter_id: chatter.id,
            sent_at: comment.created_at.naive_utc(),
            message_kind,
        });
    }

    tracing::info!(
        name: "CatchupComplete",
        chatters = chatter_map.len(),
        messages = messages.len(),
    );

    // Insert chatters and messages in a huge block to significantly increase performance.
    db.insert_many_chatters(chatter_map.values()).await?;
    db.insert_many_messages(&messages).await?;

    Ok(())
}

fn kind_from_message(message: &CommentsByVideoAndCursorMessage) -> Option<MessageKind> {
    let first_frag = &message.fragments.first()?.text;
    let last_frag = &message.fragments.last()?.text;

    Some(
        if first_frag.starts_with("+2") || last_frag.ends_with("+2") {
            MessageKind::PlusTwo
        } else if first_frag.starts_with("-2") || last_frag.ends_with("-2") {
            MessageKind::MinusTwo
        } else {
            return None;
        },
    )
}

fn timestamp_to_time(ts: &Timestamp) -> Result<DateTime> {
    DateTime::parse_from_str(ts.as_str(), "%Y-%m-%dT%H:%M:%S%.f%Z")
        .wrap_err("Failed to transform timestamp to datetime")
}
