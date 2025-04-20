use std::collections::HashMap;

use broadcaster::WatchedBroadcaster;
use eyre::{Context as _, Result, bail};
use plustwo_database::{DatabaseClient, DateTime, entities::sea_orm_active_enums::MessageKind};
use plustwo_twitch_gql::{CommentsByVideoAndCursorMessage, TwitchGqlClient};
use socket::EventSubSocket;
use state::State;
use twitch::TwitchClient;
use twitch_api::{
    TWITCH_EVENTSUB_WEBSOCKET_URL,
    eventsub::{
        Event as TwitchEvent, EventsubWebsocketData, Message, Payload,
        channel::ChannelChatMessageV1Payload,
        stream::{StreamOfflineV1Payload, StreamOnlineV1Payload},
    },
    types::Timestamp,
};

mod broadcaster;
mod socket;
mod state;
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

    let mut state = State::new(&graphql_client, env_var!("TWITCH_USER")).await?;

    let mut eventsub = EventSubSocket::connect(TWITCH_EVENTSUB_WEBSOCKET_URL.as_str()).await?;
    loop {
        if state.should_update_broadcasters() {
            state
                .update_broadcasters(&db, &graphql_client, &api_client)
                .await?;
        }

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
                state.session_id = session.id.into_owned();

                state
                    .update_broadcasters(&db, &graphql_client, &api_client)
                    .await?;

                Ok(())
            }

            // Sent if the server that the client is connected to needs to swap.
            EventsubWebsocketData::Reconnect {
                payload: twitch_api::eventsub::ReconnectPayload { session },
                ..
            } => {
                tracing::warn!(name: "RecvReconnect", session = ?session);
                state.session_id = session.id.into_owned();

                eventsub = EventSubSocket::connect(&session.reconnect_url.as_ref().map_or_else(
                    || TWITCH_EVENTSUB_WEBSOCKET_URL.to_string(),
                    ToString::to_string,
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
                    on_stream_online(&graphql_client, &db, &payload, &mut state).await
                }
                TwitchEvent::StreamOfflineV1(Payload {
                    message: Message::Notification(payload),
                    ..
                }) => {
                    tracing::info!("StreamOfflineV1({})", payload.broadcaster_user_login);
                    on_stream_offline(
                        &db,
                        &metadata.message_timestamp.into_owned(),
                        &payload,
                        &mut state,
                    )
                    .await
                }
                TwitchEvent::ChannelChatMessageV1(Payload {
                    message: Message::Notification(payload),
                    ..
                }) => {
                    on_chat_message(&db, &payload, metadata.message_timestamp.into(), &state).await
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

async fn on_stream_online(
    graphql_client: &TwitchGqlClient,
    db: &DatabaseClient,
    payload: &StreamOnlineV1Payload,
    state: &mut State,
) -> Result<()> {
    let Some(broadcaster) = state
        .broadcasters
        .get_mut(&payload.broadcaster_user_id.as_str().parse()?)
    else {
        tracing::warn!(
            "Somehow managed to recv a StreamOnline for a broadcaster who wasn't tracked ({})",
            payload.broadcaster_user_login
        );
        return Ok(());
    };

    let Some(stream) = graphql_client
        .get_stream_by_user(payload.broadcaster_user_login.as_str())
        .await?
        .stream
    else {
        bail!("Failed to find broadcast after StreamOnline for {broadcaster:?}")
    };

    db.start_broadcast(
        stream.archive_video.id.parse()?,
        broadcaster.broadcaster.id,
        stream.archive_video.title.clone(),
        timestamp_to_time(&payload.started_at)?,
    )
    .await?;

    broadcaster.current_broadcast = Some(stream);

    Ok(())
}

async fn on_stream_offline(
    db: &DatabaseClient,
    timestamp: &Timestamp,
    payload: &StreamOfflineV1Payload,
    state: &mut State,
) -> Result<()> {
    let Some(broadcaster) = state
        .broadcasters
        .get_mut(&payload.broadcaster_user_id.as_str().parse()?)
    else {
        tracing::warn!(
            "Failed to find broadcaster after StreamOffline ({})",
            payload.broadcaster_user_login
        );
        return Ok(());
    };

    db.end_broadcast(
        broadcaster.broadcaster.id,
        timestamp_to_time(timestamp)?,
        broadcaster
            .current_broadcast
            .as_ref()
            .map(|b| b.archive_video.id.parse().unwrap()),
    )
    .await?;

    broadcaster.current_broadcast = None;

    Ok(())
}

async fn on_chat_message(
    db: &DatabaseClient,
    payload: &ChannelChatMessageV1Payload,
    sent_at: twitch_api::types::Timestamp,
    state: &State,
) -> Result<()> {
    let Some(broadcaster) = state
        .broadcasters
        .get(&payload.broadcaster_user_id.as_str().parse()?)
    else {
        tracing::warn!(
            "Somehow managed to recv a message for an untracked broadcaster ({})",
            payload.broadcaster_user_login
        );
        return Ok(());
    };

    // Messages can be sent while a broadcaster isn't live, and we should skip
    // these.
    let Some(broadcast) = broadcaster.current_broadcast.as_ref() else {
        return Ok(());
    };

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
        broadcast.archive_video.id.parse()?,
        payload.chatter_user_id.as_str().parse()?,
        timestamp_to_time(&sent_at)?,
        message_kind,
    )
    .await?;

    Ok(())
}

async fn fetch_new_broadcasters(
    broadcasters: &HashMap<i64, WatchedBroadcaster>,
    db: &DatabaseClient,
    gql: &TwitchGqlClient,
) -> Result<Vec<WatchedBroadcaster>> {
    let mut new_broadcasters = Vec::new();

    for broadcaster in db.select_broadcasters().await? {
        if broadcasters.contains_key(&broadcaster.id) {
            continue;
        }

        let current_broadcast = gql
            .get_stream_by_user(&broadcaster.display_name)
            .await?
            .stream;

        new_broadcasters.push(WatchedBroadcaster {
            current_broadcast,
            broadcaster,
            is_watching: false,
        });
    }

    Ok(new_broadcasters)
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
