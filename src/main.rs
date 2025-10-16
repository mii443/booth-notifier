mod booth;
mod database;
mod event_handler;
mod filter;
mod task;

use anyhow::Result;
use database::DatabaseClient;
use event_handler::event_handler;
use poise::serenity_prelude::{self as serenity};
use tracing::info;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub db: DatabaseClient,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Load environment variables
    let token = std::env::var("BOT_TOKEN")?;
    let database_url = std::env::var("DATABASE_URL")?;
    let owners = std::env::var("BOT_OWNERS")?
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect::<std::collections::HashSet<_>>();

    // Initialize database client
    let db = DatabaseClient::new(&database_url).await?;

    // Run migrations
    db.migrate().await?;

    info!("Database connected and migrations completed");

    let framework = poise::Framework::builder()
        .setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(Data { db }) }))
        .options(poise::FrameworkOptions {
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            owners,
            ..Default::default()
        })
        .build();

    let mut client =
        serenity::ClientBuilder::new(token, serenity::GatewayIntents::non_privileged())
            .framework(framework)
            .await?;

    client.start().await?;

    Ok(())
}
