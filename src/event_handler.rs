use poise::serenity_prelude::{self as serenity, FullEvent};
use tracing::info;

use crate::{Data, Error};

pub async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::Ready { data_about_bot } => {
            info!("{} is connected!", data_about_bot.user.name);
        }
        _ => {}
    }
    Ok(())
}
