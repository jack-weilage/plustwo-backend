use std::{collections::HashMap, time::Duration};

use eyre::Context;
use indicatif::{ProgressBar, ProgressStyle};
use plustwo_database::{
    DatabaseClient,
    entities::{self, sea_orm_active_enums::MessageKind},
};
use plustwo_twitch_gql::{
    CommentsByVideoAndCursorComment, CommentsByVideoAndCursorMessage, TwitchGqlClient,
    collect_from_cursor, shared::video::TwitchVideo,
};

macro_rules! env_var {
    ($name:expr) => {
        ::std::env::var($name)
            .wrap_err_with(|| format!("Failed to find environment variable {}", $name))?
    };
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let client = TwitchGqlClient::new();
    let db = DatabaseClient::new(&env_var!("DATABASE_URL")).await?;

    let broadcaster_name = env_var!("TWITCH_BROADCASTER");
    let broadcaster = client.get_stream_by_user(&broadcaster_name).await?;

    db.insert_broadcaster(
        broadcaster.id.parse()?,
        &broadcaster_name,
        &broadcaster.profile_image_url,
    )
    .await?;

    let currently_live_video = broadcaster.stream.map(|stream| stream.archive_video.id);

    let video_bar = ProgressBar::no_length().with_style(ProgressStyle::with_template(
        "[{elapsed_precise}] {wide_bar} {pos}/{len} videos ({eta})",
    )?);

    let videos: Vec<TwitchVideo> = collect_from_cursor(async |cursor, _total, _videos| {
        client
            .get_videos_by_user_and_cursor(&broadcaster_name, cursor)
            .await
    })
    .await?;
    video_bar.set_length(videos.len() as u64);

    for video in videos {
        // If the video is currently live or the broadcast has ended, skip it.
        if currently_live_video
            .as_ref()
            .is_some_and(|id| id == &video.id)
            || db
                .get_broadcast(video.id.parse()?)
                .await?
                .is_some_and(|b| b.ended_at.is_some())
        {
            video_bar.inc(1);
            continue;
        }

        db.start_broadcast(
            video.id.parse()?,
            broadcaster.id.parse()?,
            video.title.clone(),
            video.created_at.naive_utc(),
        )
        .await?;

        let comment_bar =
            ProgressBar::new(video.length_seconds).with_style(ProgressStyle::with_template(
                "[{elapsed_precise}] {msg} {wide_bar} {pos}/{len} seconds ({eta})",
            )?);
        comment_bar.set_message(video.title);

        let mut chatters = HashMap::new();
        let mut messages = Vec::new();

        for comment in collect_from_cursor(
            async |cursor, _length, comments: &Vec<CommentsByVideoAndCursorComment>| {
                if let Some(comment) = comments.last() {
                    comment_bar
                        .set_position((comment.created_at - video.created_at).num_seconds() as u64);
                }

                client
                    .get_comments_by_video_and_cursor(&video.id, cursor)
                    .await
            },
        )
        .await?
        {
            // If the user cannot be parsed, skip the comment.
            let Some(user) = comment.commenter else {
                continue;
            };
            // If the comment is not a +2 or -2, skip it.
            let Some(message_kind) = kind_from_message(&comment.message) else {
                continue;
            };

            let chatter = entities::chatters::Model {
                id: user.id.parse()?,
                display_name: user.display_name,
            };

            chatters.insert(chatter.id, chatter.clone());
            messages.push(entities::messages::Model {
                id: comment.id,
                broadcast_id: video.id.parse()?,
                chatter_id: chatter.id,
                sent_at: comment.created_at.naive_utc(),
                message_kind,
            });
        }

        db.insert_many_chatters(chatters.values()).await?;
        db.insert_many_messages(&messages).await?;

        db.end_broadcast(
            broadcaster.id.parse()?,
            (video.created_at + Duration::from_secs(video.length_seconds)).naive_utc(),
            Some(video.id.parse()?),
        )
        .await?;

        comment_bar.finish();
        video_bar.inc(1);
    }

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
