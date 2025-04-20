use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use eyre::Result;
use plustwo_database::DatabaseClient;
use plustwo_twitch_gql::TwitchGqlClient;

use crate::{broadcaster::WatchedBroadcaster, twitch::TwitchClient};

const BROADCASTER_REFRESH_RATE: Duration = Duration::from_secs(30 * 60);

pub struct State {
    pub broadcasters: HashMap<i64, WatchedBroadcaster>,
    pub last_broadcaster_check: Instant,
    pub session_id: String,
    pub watcher_id: String,
}
impl State {
    pub async fn new(gql: &TwitchGqlClient, watcher: &str) -> Result<Self> {
        Ok(Self {
            broadcasters: HashMap::new(),
            last_broadcaster_check: Instant::now(),
            session_id: String::new(),
            watcher_id: gql.get_stream_by_user(watcher).await?.id,
        })
    }
    pub fn should_update_broadcasters(&self) -> bool {
        self.last_broadcaster_check.elapsed() > BROADCASTER_REFRESH_RATE
    }
    pub async fn update_broadcasters(
        &mut self,
        db: &DatabaseClient,
        gql: &TwitchGqlClient,
        api: &TwitchClient,
    ) -> Result<()> {
        let new_broadcasters = db.select_broadcasters().await?;
        self.last_broadcaster_check = Instant::now();

        for broadcaster in new_broadcasters {
            if self.broadcasters.contains_key(&broadcaster.id) {
                continue;
            }

            let mut broadcaster = WatchedBroadcaster {
                current_broadcast: gql
                    .get_stream_by_user(&broadcaster.display_name)
                    .await?
                    .stream,
                broadcaster,
                is_watching: false,
            };

            broadcaster
                .watch(api, &self.session_id, &self.watcher_id)
                .await?;
            broadcaster.catchup(db, gql).await?;

            self.broadcasters
                .insert(broadcaster.broadcaster.id, broadcaster);
        }

        Ok(())
    }
}
