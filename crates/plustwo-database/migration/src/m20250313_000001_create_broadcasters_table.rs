use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20250313_000001_create_broadcasters_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Broadcasters::Table)
                    .col(
                        ColumnDef::new(Broadcasters::Id)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Broadcasters::DisplayName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Broadcasters::ProfileImageUrl)
                            .string()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Broadcasters::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
pub enum Broadcasters {
    Table,
    Id,
    DisplayName,
    ProfileImageUrl,
}
