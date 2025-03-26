use extension::postgres::Type;
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20250313_000004_create_message_kind_type"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(MessageKind::MessageKind)
                    .values([MessageKind::PlusTwo, MessageKind::MinusTwo])
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_type(Type::drop().name(MessageKind::MessageKind).to_owned())
            .await
    }
}

pub enum MessageKind {
    MessageKind,
    PlusTwo,
    MinusTwo,
}
impl Iden for MessageKind {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(
            s,
            "{}",
            match self {
                MessageKind::MessageKind => "message_kind",
                MessageKind::PlusTwo => "plus_two",
                MessageKind::MinusTwo => "minus_two",
            }
        )
        .unwrap();
    }
}
