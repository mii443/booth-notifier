mod booth;
mod commands;
mod database;
mod event_handler;
mod filter;
mod task;

use anyhow::Result;
use database::DatabaseClient;
use event_handler::event_handler;
use poise::{
    serenity_prelude::{self as serenity},
    PrefixFrameworkOptions,
};
use tracing::info;

use crate::commands::{
    avatar::avatar_command,
    register::{register, register_server},
};

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
    let prefix = std::env::var("BOT_PREFIX").unwrap_or_else(|_| "!".to_string());

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
            commands: vec![avatar_command(), register(), register_server()],
            prefix_options: PrefixFrameworkOptions {
                prefix: Some(prefix),
                ..Default::default()
            },
            ..Default::default()
        })
        .build();

    let mut client = serenity::ClientBuilder::new(
        token,
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT,
    )
    .framework(framework)
    .await?;

    client.start().await?;

    Ok(())
}
