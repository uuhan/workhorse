pub use sea_orm_migration::prelude::*;

mod m20250104_174457_create_user;
mod m20250125_083941_create_ssh_pk;
mod m20260307_090000_add_user_role_and_enable;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250104_174457_create_user::Migration),
            Box::new(m20250125_083941_create_ssh_pk::Migration),
            Box::new(m20260307_090000_add_user_role_and_enable::Migration),
        ]
    }
}
