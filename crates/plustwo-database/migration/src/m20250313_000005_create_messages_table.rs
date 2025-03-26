use sea_orm_migration::prelude::*;

use crate::{
    m20250313_000002_create_broadcasts_table::Broadcasts,
    m20250313_000003_create_chatters_table::Chatters,
    m20250313_000004_create_message_kind_type::MessageKind,
};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20250313_000005_create_messages_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Messages::Table)
                    .col(ColumnDef::new(Messages::Id).uuid().not_null().primary_key())
                    .col(
                        ColumnDef::new(Messages::BroadcastId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Messages::ChatterId).big_integer().not_null())
                    .col(ColumnDef::new(Messages::SentAt).timestamp().not_null())
                    .col(
                        ColumnDef::new(Messages::MessageKind)
                            .custom(MessageKind::MessageKind)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-broadcast-id")
                            .from(Messages::Table, Messages::BroadcastId)
                            .to(Broadcasts::Table, Broadcasts::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-chatter-id")
                            .from(Messages::Table, Messages::ChatterId)
                            .to(Chatters::Table, Chatters::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Messages::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
pub enum Messages {
    Table,

    Id,
    BroadcastId,
    ChatterId,

    SentAt,

    MessageKind,
}
