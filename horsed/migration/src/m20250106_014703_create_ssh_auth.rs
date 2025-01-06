use super::m20250104_174457_create_user::User;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SshAuth::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SshAuth::Method).string().not_null())
                    .col(ColumnDef::new(SshAuth::Key).string().not_null())
                    .col(ColumnDef::new(SshAuth::UserId).integer().not_null())
                    .primary_key(Index::create().col(SshAuth::Method).col(SshAuth::Key))
                    .foreign_key(
                        ForeignKey::create()
                            .from(SshAuth::Table, SshAuth::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SshAuth::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SshAuth {
    Table,
    Method,
    Key,
    UserId,
}
