//! `SeaORM` Entity, @generated by sea-orm-codegen 1.1.7

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "broadcasts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub broadcaster_id: i64,
    pub title: String,
    pub started_at: DateTime,
    pub ended_at: Option<DateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::broadcasters::Entity",
        from = "Column::BroadcasterId",
        to = "super::broadcasters::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Broadcasters,
    #[sea_orm(has_many = "super::messages::Entity")]
    Messages,
}

impl Related<super::broadcasters::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Broadcasters.def()
    }
}

impl Related<super::messages::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Messages.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
