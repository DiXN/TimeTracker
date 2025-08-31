use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // This migration is now a no-op since we're consolidating checkpoint_durations with checkpoints
        // The functionality is moved to the checkpoints table directly
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // This would recreate the checkpoint_durations table if needed
        manager
            .create_table(
                Table::create()
                    .table(CheckpointDurations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CheckpointDurations::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CheckpointDurations::CheckpointId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CheckpointDurations::AppId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CheckpointDurations::Duration)
                            .integer()
                            .null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CheckpointDurations::SessionsCount)
                            .integer()
                            .null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CheckpointDurations::LastUpdated)
                            .timestamp()
                            .null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_checkpoint_durations_checkpoint_id")
                            .from(
                                CheckpointDurations::Table,
                                CheckpointDurations::CheckpointId,
                            )
                            .to(Checkpoints::Table, Checkpoints::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_checkpoint_durations_app_id")
                            .from(CheckpointDurations::Table, CheckpointDurations::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum CheckpointDurations {
    Table,
    Id,
    CheckpointId,
    AppId,
    Duration,
    SessionsCount,
    LastUpdated,
}

#[derive(DeriveIden)]
enum Checkpoints {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
