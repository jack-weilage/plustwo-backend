use sea_orm_migration::prelude::*;

use crate::m20250313_000001_create_broadcasters_table::Broadcasters;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20250313_000002_create_broadcasts_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Broadcasts::Table)
                    .col(
                        ColumnDef::new(Broadcasts::Id)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Broadcasts::BroadcasterId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Broadcasts::Title).string().not_null())
                    .col(ColumnDef::new(Broadcasts::StartedAt).timestamp().not_null())
                    .col(ColumnDef::new(Broadcasts::EndedAt).timestamp())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-broadcaster-id")
                            .from(Broadcasts::Table, Broadcasts::BroadcasterId)
                            .to(Broadcasters::Table, Broadcasters::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Broadcasts::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
pub enum Broadcasts {
    Table,
    Id,
    BroadcasterId,
    Title,
    StartedAt,
    EndedAt,
}
