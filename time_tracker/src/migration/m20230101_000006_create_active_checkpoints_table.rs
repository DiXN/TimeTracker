use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // This migration is now a no-op since we're consolidating active_checkpoints with checkpoints
        // The functionality is moved to the checkpoints table directly
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // This would recreate the active_checkpoints table if needed
        manager
            .create_table(
                Table::create()
                    .table(ActiveCheckpoints::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ActiveCheckpoints::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ActiveCheckpoints::CheckpointId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ActiveCheckpoints::ActivatedAt)
                            .timestamp()
                            .null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(ActiveCheckpoints::AppId)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_active_checkpoints_checkpoint_id")
                            .from(ActiveCheckpoints::Table, ActiveCheckpoints::CheckpointId)
                            .to(Checkpoints::Table, Checkpoints::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_active_checkpoints_app_id")
                            .from(ActiveCheckpoints::Table, ActiveCheckpoints::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ActiveCheckpoints {
    Table,
    Id,
    CheckpointId,
    ActivatedAt,
    AppId,
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
