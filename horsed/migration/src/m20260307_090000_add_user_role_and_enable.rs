use super::m20250104_174457_create_user::User;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ROLE_ADMIN: &str = "admin";
const ROLE_USER: &str = "user";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("role"))
                            .string()
                            .not_null()
                            .default(ROLE_USER),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("enabled"))
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("ssh_pk"))
                    .add_column(
                        ColumnDef::new(Alias::new("enabled"))
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("ssh_pk"))
                    .add_column(ColumnDef::new(Alias::new("comment")).string().null())
                    .to_owned(),
            )
            .await?;

        // Keep existing deployments operable: ensure at least one admin user exists.
        manager
            .get_connection()
            .execute_unprepared(&format!(
                r#"
UPDATE "user"
SET "role" = '{admin}'
WHERE "id" = (SELECT MIN("id") FROM "user")
  AND NOT EXISTS (
    SELECT 1 FROM "user" WHERE "role" = '{admin}'
  )
"#,
                admin = ROLE_ADMIN
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("ssh_pk"))
                    .drop_column(Alias::new("comment"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("ssh_pk"))
                    .drop_column(Alias::new("enabled"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .drop_column(Alias::new("enabled"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .drop_column(Alias::new("role"))
                    .to_owned(),
            )
            .await
    }
}
