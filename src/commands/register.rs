use crate::{database::NewDiscordGuild, Context, Error};

#[poise::command(prefix_command, owners_only)]
pub async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;

    ctx.say("Registered commands!").await?;
    Ok(())
}

#[poise::command(slash_command, guild_only, owners_only)]
pub async fn register_server(
    ctx: Context<'_>,
    sfw_category_id: String,
    nsfw_category_id: String,
    sfw_fallback_id: String,
    nsfw_fallback_id: String,
) -> Result<(), Error> {
    let sfw_category_id: u64 = sfw_category_id.parse().map_err(|_| {
        "Invalid sfw_category_id: must be a valid number"
    })?;
    let nsfw_category_id: u64 = nsfw_category_id.parse().map_err(|_| {
        "Invalid nsfw_category_id: must be a valid number"
    })?;
    let sfw_fallback_id: u64 = sfw_fallback_id.parse().map_err(|_| {
        "Invalid sfw_fallback_id: must be a valid number"
    })?;
    let nsfw_fallback_id: u64 = nsfw_fallback_id.parse().map_err(|_| {
        "Invalid nsfw_fallback_id: must be a valid number"
    })?;

    let db = ctx.data().db.clone();
    let guild_id = ctx.guild_id().ok_or("This command must be used in a guild")?;

    let new_guild = NewDiscordGuild {
        guild_id: guild_id.get() as i64,
        name: guild_id
            .name(ctx.cache())
            .unwrap_or("Unknown".to_string())
            .to_string(),
        fallback_channel_id: Some(sfw_fallback_id as i64),
        fallback_nsfw_channel_id: Some(nsfw_fallback_id as i64),
        general_category_id: Some(sfw_category_id as i64),
        nsfw_category_id: Some(nsfw_category_id as i64),
    };

    db.upsert_discord_guild(new_guild).await?;

    ctx.reply("Registered this server").await?;

    Ok(())
}
