mod booth;
mod commands;
mod database;
mod event_handler;
mod filter;
mod task;
mod web;

use anyhow::Result;
use database::DatabaseClient;
use event_handler::event_handler;
use poise::{
    PrefixFrameworkOptions,
    serenity_prelude::{self as serenity},
};
use tracing::{error, info};

use crate::{
    booth::item::BoothDbClient,
    commands::{
        avatar::avatar_command,
        notification::booth_command,
        register::{register, register_server},
    },
};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub db: DatabaseClient,
    pub booth_db: BoothDbClient,
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
    let booth_db_database_url = std::env::var("BOOTH_DB_DATABASE_URL")?;
    let booth_db_recent_item_limit = std::env::var("BOOTH_DB_RECENT_ITEM_LIMIT")
        .unwrap_or_else(|_| "120".to_string())
        .parse::<i64>()
        .unwrap_or(120);
    let owner_ids = std::env::var("BOT_OWNERS")?
        .split(',')
        .map(|s| s.parse::<u64>())
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse BOT_OWNERS: {}", e))?;
    let owners = owner_ids
        .iter()
        .map(|id| serenity::UserId::new(*id))
        .collect::<std::collections::HashSet<_>>();
    let prefix = std::env::var("BOT_PREFIX").unwrap_or_else(|_| "!".to_string());

    // Initialize database client
    let db = DatabaseClient::new(&database_url).await?;
    let booth_db = BoothDbClient::new(&booth_db_database_url, booth_db_recent_item_limit).await?;

    // Run migrations
    db.migrate().await?;

    info!("Database connected and migrations completed");

    let web_task = match web::WebConfig::from_env(db.clone(), token.clone(), owner_ids.clone())? {
        Some(config) => Some(tokio::spawn(async move {
            if let Err(err) = web::serve(config).await {
                error!(error = %err, "web UI stopped");
            }
        })),
        None => {
            info!("WEB_BIND is not set; web UI disabled");
            None
        }
    };

    let framework = poise::Framework::builder()
        .setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(Data { db, booth_db }) }))
        .options(poise::FrameworkOptions {
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            owners,
            commands: vec![
                avatar_command(),
                booth_command(),
                register(),
                register_server(),
            ],
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

    if let Some(web_task) = web_task {
        web_task.abort();
    }

    Ok(())
}
