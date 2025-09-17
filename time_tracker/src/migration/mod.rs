pub use sea_orm_migration::prelude::*;

mod m20230101_000001_create_apps_table;
mod m20230101_000002_create_timeline_table;
mod m20230101_000003_create_checkpoints_table;
mod m20230101_000004_create_timeline_checkpoints_table;
mod m20230101_000005_create_checkpoint_durations_table;
mod m20230101_000006_create_active_checkpoints_table;
mod m20230101_000007_create_process_aliases_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20230101_000001_create_apps_table::Migration),
            Box::new(m20230101_000002_create_timeline_table::Migration),
            Box::new(m20230101_000003_create_checkpoints_table::Migration),
            Box::new(m20230101_000004_create_timeline_checkpoints_table::Migration),
            Box::new(m20230101_000005_create_checkpoint_durations_table::Migration),
            Box::new(m20230101_000006_create_active_checkpoints_table::Migration),
            Box::new(m20230101_000007_create_process_aliases_table::Migration),
        ]
    }
}
