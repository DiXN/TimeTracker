use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Timeline::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Timeline::Id)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Timeline::Date).date().not_null())
                    .col(ColumnDef::new(Timeline::Duration).integer())
                    .col(ColumnDef::new(Timeline::AppId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_timeline_app_id")
                            .from(Timeline::Table, Timeline::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Timeline::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Timeline {
    Table,
    Id,
    Date,
    Duration,
    AppId,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
