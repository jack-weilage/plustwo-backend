use entities::sea_orm_active_enums::MessageKind;
use entities::{
    broadcasters::Entity as Broadcasters, broadcasts::Entity as Broadcasts,
    chatters::Entity as Chatters, messages::Entity as Messages,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityOrSelect, IntoActiveModel, QueryFilter};
use sea_orm::{
    ActiveValue::{Set, Unchanged},
    Database, DatabaseConnection, EntityTrait as _,
    sea_query::OnConflict,
};

pub use sea_orm::prelude::{DateTime, Uuid};

pub mod entities;

pub struct DatabaseClient {
    db: DatabaseConnection,
}
impl DatabaseClient {
    /// Constructs a new database client and connects to the URL.
    pub async fn new(url: &str) -> Result<Self, sea_orm::DbErr> {
        let db = Database::connect(url).await?;

        Ok(Self { db })
    }

    /// Retrieves a complete list of broadcasters.
    pub async fn select_broadcasters(
        &self,
    ) -> Result<Vec<entities::broadcasters::Model>, sea_orm::DbErr> {
        Broadcasters::find().all(&self.db).await
    }

    /// Inserts a new broadcaster into the database, updating if the entry already exists.
    pub async fn insert_broadcaster(
        &self,
        id: i64,
        display_name: &str,
        profile_image_url: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let broadcaster = entities::broadcasters::ActiveModel {
            id: Set(id),
            display_name: Set(display_name.to_string()),
            profile_image_url: Set(Some(profile_image_url.to_string())),
        };

        Broadcasters::insert(broadcaster)
            .on_conflict(
                OnConflict::column(entities::broadcasters::Column::Id)
                    .update_columns([
                        entities::broadcasters::Column::DisplayName,
                        entities::broadcasters::Column::ProfileImageUrl,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }

    /// Inserts a new chatter, updating display name if the chatter already exists.
    pub async fn insert_chatter(
        &self,
        id: i64,
        display_name: String,
    ) -> Result<(), sea_orm::DbErr> {
        let chatter = entities::chatters::ActiveModel {
            id: Set(id),
            display_name: Set(display_name),
        };

        Chatters::insert(chatter)
            .on_conflict(
                OnConflict::column(entities::chatters::Column::Id)
                    .update_column(entities::chatters::Column::DisplayName)
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn insert_many_chatters(
        &self,
        chatters: impl Iterator<Item = &entities::chatters::Model>,
    ) -> Result<(), sea_orm::DbErr> {
        Chatters::insert_many(chatters.cloned().map(IntoActiveModel::into_active_model))
            .on_conflict_do_nothing()
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn insert_message(
        &self,
        id: Uuid,
        broadcast_id: i64,
        chatter_id: i64,
        sent_at: DateTime,
        message_kind: MessageKind,
    ) -> Result<(), sea_orm::DbErr> {
        let message = entities::messages::ActiveModel {
            id: Set(id),
            broadcast_id: Set(broadcast_id),
            chatter_id: Set(chatter_id),
            sent_at: Set(sent_at),
            message_kind: Set(message_kind),
        };

        Messages::insert(message)
            .on_conflict_do_nothing()
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn insert_many_messages(
        &self,
        messages: &[entities::messages::Model],
    ) -> Result<(), sea_orm::DbErr> {
        Messages::insert_many(
            messages
                .iter()
                .cloned()
                .map(IntoActiveModel::into_active_model),
        )
        .on_conflict_do_nothing()
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// Inserts a new broadcast attributed to the broadcaster
    pub async fn start_broadcast(
        &self,
        broadcast_id: i64,
        broadcaster_id: i64,
        title: String,
        started_at: DateTime,
    ) -> Result<(), sea_orm::DbErr> {
        let broadcast = entities::broadcasts::ActiveModel {
            id: Set(broadcast_id),
            broadcaster_id: Set(broadcaster_id),
            title: Set(title),
            started_at: Set(started_at),
            ..Default::default()
        };

        Broadcasts::insert(broadcast)
            .on_conflict(
                OnConflict::column(entities::broadcasts::Column::Id)
                    .update_column(entities::broadcasts::Column::StartedAt)
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }

    /// Updates the last broadcast from the specified user to end at the specified time.
    pub async fn end_broadcast(
        &self,
        broadcaster_id: i64,
        ended_at: DateTime,
        broadcast_id: Option<i64>,
    ) -> Result<(), sea_orm::DbErr> {
        // If a broadcast ID is provided, we can use it to find the broadcast.
        // Otherwise, we need to find the most recent broadcast that has not ended yet.

        let broadcast = if let Some(broadcast_id) = broadcast_id {
            Broadcasts::find_by_id(broadcast_id).one(&self.db).await?
        } else {
            Broadcasts::find()
                .filter(entities::broadcasts::Column::EndedAt.is_null())
                .filter(entities::broadcasts::Column::BroadcasterId.eq(broadcaster_id))
                .one(&self.db)
                .await?
        };

        // TODO: Better error handling
        let mut broadcast = broadcast.unwrap().into_active_model();
        broadcast.ended_at = Set(Some(ended_at));

        broadcast.update(&self.db).await?;

        Ok(())
    }

    pub async fn get_broadcast(
        &self,
        broadcast_id: i64,
    ) -> Result<Option<entities::broadcasts::Model>, sea_orm::DbErr> {
        Broadcasts::find_by_id(broadcast_id).one(&self.db).await
    }
}
