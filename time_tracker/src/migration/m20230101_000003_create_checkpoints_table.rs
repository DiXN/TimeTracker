use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Checkpoints::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Checkpoints::Id)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Checkpoints::Name).string().not_null())
                    .col(ColumnDef::new(Checkpoints::Description).text().null())
                    .col(
                        ColumnDef::new(Checkpoints::CreatedAt)
                            .timestamp()
                            .null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Checkpoints::ValidFrom)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Checkpoints::Color).string().null())
                    .col(ColumnDef::new(Checkpoints::AppId).integer().not_null())
                    .col(
                        ColumnDef::new(Checkpoints::IsActive)
                            .boolean()
                            .null()
                            .default(false),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_checkpoints_app_id")
                            .from(Checkpoints::Table, Checkpoints::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Checkpoints::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Checkpoints {
    Table,
    Id,
    Name,
    Description,
    CreatedAt,
    ValidFrom,
    Color,
    AppId,
    IsActive,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
