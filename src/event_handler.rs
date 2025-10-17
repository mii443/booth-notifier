use poise::serenity_prelude::{self as serenity, FullEvent};
use tracing::{error, info};

use crate::{
    task::{NotifyTask, ScrapingTask},
    Data, Error,
};

pub async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::Ready { data_about_bot } => {
            ready_handler(ctx, data_about_bot, data).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn ready_handler(
    ctx: &serenity::Context,
    ready: &serenity::Ready,
    data: &Data,
) -> Result<(), Error> {
    info!("{} is connected!", ready.user.name);

    let ctx = ctx.clone();
    let database_client = data.db.clone();
    tokio::spawn(async move {
        let check_interval = std::env::var("CHECK_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<u64>()
            .unwrap_or(60);
        let check_interval = std::time::Duration::from_secs(check_interval);
        let fetch_interval = std::env::var("FETCH_INTERVAL_MILLISECONDS")
            .unwrap_or_else(|_| "500".to_string())
            .parse::<u64>()
            .unwrap_or(500);
        let fetch_interval = std::time::Duration::from_millis(fetch_interval);

        let mut scraping_task = ScrapingTask::new(fetch_interval);
        let notify_task = NotifyTask::new();

        loop {
            let items = match scraping_task.run(&database_client).await {
                Ok(items) => items,
                Err(e) => {
                    error!("Error during scraping task: {:?}", e);
                    vec![]
                }
            };

            if let Err(e) = notify_task.notify(&ctx, &database_client, &items).await {
                error!("Error during notify task: {:?}", e);
            }

            tokio::time::sleep(check_interval).await;
        }
    });

    Ok(())
}
