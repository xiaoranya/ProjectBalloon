use std::env;

use anyhow::{Context, Result, bail};
use project_balloon_api::bootstrap::{BootstrapAdmin, bootstrap_super_admin};
use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = required_env("DATABASE_URL")?;
    let username = required_env("PROJECT_BALLOON_BOOTSTRAP_ADMIN_USERNAME")?;
    let display_name = required_env("PROJECT_BALLOON_BOOTSTRAP_ADMIN_DISPLAY_NAME")?;
    let password = required_env("PROJECT_BALLOON_BOOTSTRAP_ADMIN_PASSWORD")?;
    let admin = BootstrapAdmin::new(username, display_name, password)
        .context("invalid administrator bootstrap input")?;

    let database = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("failed to connect to PostgreSQL")?;
    MIGRATOR.run(&database).await.context("failed to run PostgreSQL migrations")?;
    let user_id = bootstrap_super_admin(&database, admin)
        .await
        .context("failed to bootstrap the first super administrator")?;
    database.close().await;

    println!("created initial super administrator with user ID {user_id}");
    Ok(())
}

fn required_env(name: &'static str) -> Result<String> {
    let value = env::var(name).with_context(|| format!("{name} is required"))?;
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(value)
}
