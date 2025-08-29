use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Apps::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Apps::Id).integer().not_null().primary_key())
                    .col(ColumnDef::new(Apps::Duration).integer())
                    .col(ColumnDef::new(Apps::Launches).integer())
                    .col(ColumnDef::new(Apps::LongestSession).integer())
                    .col(ColumnDef::new(Apps::Name).string().null())
                    .col(ColumnDef::new(Apps::ProductName).string().null())
                    .col(ColumnDef::new(Apps::LongestSessionOn).date().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Apps::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
    Duration,
    Launches,
    LongestSession,
    Name,
    ProductName,
    LongestSessionOn,
}
