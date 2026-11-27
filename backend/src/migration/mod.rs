use sea_orm_migration::prelude::*;

mod m20261024_000001_init;
mod m20261025_000001_prototype2;
mod m20261026_000001_links_version;
mod m20261026_000002_totp;
mod m20261026_000003_user_theme;
mod m20261027_000001_draft_layout;
mod m20261027_000002_pinned;
mod m20261028_000001_logs;
mod m20261029_000001_site_settings;
mod m20261030_000001_database;
mod m20261030_000002_snippets;
mod m20261125_000001_guest_interactions;
mod m20261125_000002_visibility;
mod m20261127_000001_lock;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261024_000001_init::Migration),
            Box::new(m20261025_000001_prototype2::Migration),
            Box::new(m20261026_000001_links_version::Migration),
            Box::new(m20261026_000002_totp::Migration),
            Box::new(m20261026_000003_user_theme::Migration),
            Box::new(m20261027_000001_draft_layout::Migration),
            Box::new(m20261027_000002_pinned::Migration),
            Box::new(m20261028_000001_logs::Migration),
            Box::new(m20261029_000001_site_settings::Migration),
            Box::new(m20261030_000001_database::Migration),
            Box::new(m20261030_000002_snippets::Migration),
            Box::new(m20261125_000001_guest_interactions::Migration),
            Box::new(m20261125_000002_visibility::Migration),
            Box::new(m20261127_000001_lock::Migration),
        ]
    }
}
