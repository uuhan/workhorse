pub use sea_orm_migration::prelude::*;

mod m20250104_174457_create_user;
mod m20250106_014703_create_ssh_auth;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250104_174457_create_user::Migration),
            Box::new(m20250106_014703_create_ssh_auth::Migration),
        ]
    }
}
