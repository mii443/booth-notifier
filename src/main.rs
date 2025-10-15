mod booth;
mod event_handler;

use anyhow::Result;
use event_handler::event_handler;
use poise::serenity_prelude::{self as serenity};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data;

#[tokio::main]
async fn main() -> Result<()> {
    let token = std::env::var("BOT_TOKEN")?;
    let owners = std::env::var("BOT_OWNERS")?
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect::<std::collections::HashSet<_>>();

    let framework = poise::Framework::builder()
        .setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(Data) }))
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
