use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TimelineCheckpoints::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TimelineCheckpoints::Id)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TimelineCheckpoints::TimelineId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TimelineCheckpoints::CheckpointId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TimelineCheckpoints::CreatedAt)
                            .timestamp()
                            .null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_timeline_checkpoints_timeline_id")
                            .from(TimelineCheckpoints::Table, TimelineCheckpoints::TimelineId)
                            .to(Timeline::Table, Timeline::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_timeline_checkpoints_checkpoint_id")
                            .from(
                                TimelineCheckpoints::Table,
                                TimelineCheckpoints::CheckpointId,
                            )
                            .to(Checkpoints::Table, Checkpoints::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TimelineCheckpoints::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum TimelineCheckpoints {
    Table,
    Id,
    TimelineId,
    CheckpointId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Timeline {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Checkpoints {
    Table,
    Id,
}
