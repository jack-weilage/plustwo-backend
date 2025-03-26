pub use sea_orm_migration::prelude::*;

mod m20250313_000001_create_broadcasters_table;
mod m20250313_000002_create_broadcasts_table;
mod m20250313_000003_create_chatters_table;
mod m20250313_000004_create_message_kind_type;
mod m20250313_000005_create_messages_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250313_000001_create_broadcasters_table::Migration),
            Box::new(m20250313_000002_create_broadcasts_table::Migration),
            Box::new(m20250313_000003_create_chatters_table::Migration),
            Box::new(m20250313_000004_create_message_kind_type::Migration),
            Box::new(m20250313_000005_create_messages_table::Migration),
        ]
    }
}
