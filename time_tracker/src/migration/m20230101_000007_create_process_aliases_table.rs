use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProcessAliases::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProcessAliases::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ProcessAliases::ProcessName)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ProcessAliases::AppId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_process_aliases_app_id")
                            .from(ProcessAliases::Table, ProcessAliases::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ProcessAliases::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ProcessAliases {
    Table,
    Id,
    ProcessName,
    AppId,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
