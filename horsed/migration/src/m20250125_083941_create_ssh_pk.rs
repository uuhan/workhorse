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
                    .table(SshPk::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SshPk::Alg).string().not_null())
                    .col(ColumnDef::new(SshPk::Key).string().not_null())
                    .col(ColumnDef::new(SshPk::UserId).integer().not_null())
                    .primary_key(Index::create().col(SshPk::Alg).col(SshPk::Key))
                    .foreign_key(
                        ForeignKey::create()
                            .from(SshPk::Table, SshPk::UserId)
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
            .drop_table(Table::drop().table(SshPk::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SshPk {
    Table,
    Alg,
    Key,
    UserId,
}
