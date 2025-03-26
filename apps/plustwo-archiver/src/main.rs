use std::{collections::HashMap, time::Duration};

use eyre::Context;
use indicatif::{ProgressBar, ProgressStyle};
use plustwo_database::{
    DatabaseClient,
    entities::{self, sea_orm_active_enums::MessageKind},
};
use plustwo_twitch_gql::TwitchGqlClient;

macro_rules! env_var {
    ($name:expr) => {
        ::std::env::var($name)
            .wrap_err_with(|| format!("Failed to find environment variable {}", $name))?
            .as_str()
    };
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let client = TwitchGqlClient::new();
    let db = DatabaseClient::new(env_var!("DATABASE_URL")).await?;

    let broadcaster = client
        .get_stream_by_user(env_var!("TWITCH_BROADCASTER"))
        .await?;

    db.insert_broadcaster(
        broadcaster.id.parse()?,
        env_var!("TWITCH_BROADCASTER"),
        &broadcaster.profile_image_url,
    )
    .await?;

    let currently_live_video = broadcaster.stream.map(|stream| stream.archive_video.id);

    let video_bar = ProgressBar::no_length().with_style(ProgressStyle::with_template(
        "[{elapsed_precise}] {wide_bar} {pos}/{len} videos ({eta})",
    )?);

    let mut video_cursor = None;
    loop {
        let videos = client
            .get_videos_by_user_and_cursor(env_var!("TWITCH_BROADCASTER"), video_cursor.clone())
            .await?;

        video_bar.set_length(videos.total_count as u64);

        for video in videos.edges {
            video_cursor = video.cursor;

            if currently_live_video
                .as_ref()
                .is_some_and(|id| id == &video.node.id)
            {
                video_bar.set_length(video_bar.length().unwrap() - 1);

                continue;
            }

            db.start_broadcast(
                video.node.id.parse()?,
                broadcaster.id.parse()?,
                video.node.title,
                video.node.created_at.naive_utc(),
            )
            .await?;

            let comment_bar = ProgressBar::new(video.node.length_seconds).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos}/{len} seconds ({eta})",
                )?,
            );

            let mut chatters = HashMap::new();
            let mut messages = Vec::new();

            let mut comment_cursor = None;
            loop {
                let comments = client
                    .get_comments_by_video_and_cursor(&video.node.id, comment_cursor.clone())
                    .await?;

                for comment in comments.edges {
                    comment_cursor = comment.cursor;

                    let Some(user) = comment.node.commenter else {
                        continue;
                    };

                    comment_bar.set_position(
                        (comment.node.created_at - video.node.created_at).num_seconds() as u64,
                    );

                    // Some users don't show up. Maybe they've deleted their accounts or been
                    // banned?
                    let first_chunk = &comment.node.message.fragments.first().unwrap().text;
                    let last_chunk = &comment.node.message.fragments.last().unwrap().text;

                    let message_kind =
                        if first_chunk.starts_with("+2") || last_chunk.ends_with("+2") {
                            MessageKind::PlusTwo
                        } else if first_chunk.starts_with("-2") || last_chunk.ends_with("-2") {
                            MessageKind::MinusTwo
                        } else {
                            continue;
                        };

                    let chatter = entities::chatters::Model {
                        id: user.id.parse()?,
                        display_name: user.display_name,
                    };

                    chatters.insert(chatter.id, chatter.clone());
                    messages.push(entities::messages::Model {
                        id: comment.node.id,
                        broadcast_id: video.node.id.parse()?,
                        chatter_id: chatter.id,
                        sent_at: comment.node.created_at.naive_utc(),
                        message_kind,
                    });
                }

                if !comments.page_info.has_next_page {
                    break;
                }
            }

            db.insert_many_chatters(chatters.values()).await?;
            db.insert_many_messages(&messages).await?;

            db.end_broadcast(
                video.node.id.parse()?,
                (video.node.created_at + Duration::from_secs(video.node.length_seconds))
                    .naive_utc(),
            )
            .await?;

            comment_bar.finish();
            video_bar.inc(1);
        }

        if !videos.page_info.has_next_page {
            break;
        }
    }

    Ok(())
}
