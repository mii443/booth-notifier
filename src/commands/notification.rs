use poise::serenity_prelude::ChannelId;

use crate::{
    database::NewNotificationFilter,
    filter::Filter,
    Context, Error,
};

// Main command
#[poise::command(
    slash_command,
    rename = "booth",
    guild_only,
    subcommands("filter", "channel"),
    subcommand_required,
    owners_only
)]
pub async fn booth_command(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

// ==================== Filter Subcommand Group ====================

#[poise::command(
    slash_command,
    rename = "filter",
    guild_only,
    subcommands("filter_add", "filter_list", "filter_view", "filter_delete"),
    subcommand_required,
    owners_only
)]
pub async fn filter(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add a filter (YAML format)
#[poise::command(slash_command, rename = "add", guild_only, ephemeral, owners_only)]
pub async fn filter_add(
    ctx: Context<'_>,
    #[description = "Filter definition in YAML format"] yaml: String,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    // Parse YAML
    let filter: Filter = match serde_yaml::from_str(&yaml) {
        Ok(f) => f,
        Err(e) => {
            ctx.say(format!("❌ YAML parse error: {}", e)).await?;
            return Ok(());
        }
    };

    // Validation: filter must not be empty
    if filter.groups.is_empty() {
        ctx.say("❌ Filter must have at least one group").await?;
        return Ok(());
    }

    // Save filter
    let saved_filter = db
        .create_notification_filter(NewNotificationFilter {
            rule_yaml: serde_yaml::to_string(&filter)?,
        })
        .await?;

    ctx.say(format!(
        "✅ Filter created successfully!\nFilter ID: `{}`\n\nYou can now assign this filter to a channel using:\n`/booth channel set-filter <channel> {}`",
        saved_filter.id, saved_filter.id
    ))
    .await?;

    Ok(())
}

/// List all filters
#[poise::command(slash_command, rename = "list", guild_only, ephemeral, owners_only)]
pub async fn filter_list(ctx: Context<'_>) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    let filters = db.get_all_notification_filters().await?;

    if filters.is_empty() {
        ctx.say("No filters found.").await?;
        return Ok(());
    }

    // Format filter list
    let mut message = format!("**📋 Notification Filters ({} total)**\n\n", filters.len());

    for filter in filters.iter().take(10) {
        // Show only first 10
        let preview = if filter.rule_yaml.len() > 100 {
            format!("{}...", &filter.rule_yaml[..100])
        } else {
            filter.rule_yaml.clone()
        };

        message.push_str(&format!(
            "**Filter ID:** `{}`\n**Created:** <t:{}:R>\n```yaml\n{}\n```\n\n",
            filter.id,
            filter.created_at.unix_timestamp(),
            preview
        ));
    }

    if filters.len() > 10 {
        message.push_str(&format!("\n*...and {} more filters*", filters.len() - 10));
    }

    ctx.say(message).await?;

    Ok(())
}

/// View filter details
#[poise::command(slash_command, rename = "view", guild_only, ephemeral, owners_only)]
pub async fn filter_view(
    ctx: Context<'_>,
    #[description = "Filter ID to view"] filter_id: i64,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    let filter = db.get_notification_filter(filter_id).await?;

    match filter {
        Some(f) => {
            // Check if linked to channels
            let channels = db.get_channels_by_guild(ctx.guild_id().unwrap().get() as i64).await?;
            let linked_channels: Vec<_> = channels
                .iter()
                .filter(|c| c.filter_id == Some(filter_id))
                .collect();

            let channels_info = if linked_channels.is_empty() {
                "No channels linked to this filter".to_string()
            } else {
                linked_channels
                    .iter()
                    .map(|c| format!("<#{}>", c.channel_id))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            ctx.say(format!(
                "**🔍 Filter Details**\n\n**Filter ID:** `{}`\n**Created:** <t:{}:R>\n**Linked Channels:** {}\n\n**YAML Definition:**\n```yaml\n{}\n```",
                f.id,
                f.created_at.unix_timestamp(),
                channels_info,
                f.rule_yaml
            ))
            .await?;
        }
        None => {
            ctx.say(format!("❌ Filter with ID `{}` not found", filter_id))
                .await?;
        }
    }

    Ok(())
}

/// Delete a filter
#[poise::command(slash_command, rename = "delete", guild_only, ephemeral, owners_only)]
pub async fn filter_delete(
    ctx: Context<'_>,
    #[description = "Filter ID to delete"] filter_id: i64,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    // Check if linked to channels
    let channels = db.get_channels_by_guild(ctx.guild_id().unwrap().get() as i64).await?;
    let linked_channels: Vec<_> = channels
        .iter()
        .filter(|c| c.filter_id == Some(filter_id))
        .collect();

    if !linked_channels.is_empty() {
        let channel_list = linked_channels
            .iter()
            .map(|c| format!("<#{}>", c.channel_id))
            .collect::<Vec<_>>()
            .join(", ");

        ctx.say(format!(
            "⚠️ Cannot delete filter `{}` because it is linked to the following channels:\n{}\n\nPlease clear the filter from these channels first using `/booth channel clear-filter`",
            filter_id, channel_list
        ))
        .await?;
        return Ok(());
    }

    let deleted = db.delete_notification_filter(filter_id).await?;

    if deleted {
        ctx.say(format!("✅ Filter `{}` deleted successfully", filter_id))
            .await?;
    } else {
        ctx.say(format!("❌ Filter with ID `{}` not found", filter_id))
            .await?;
    }

    Ok(())
}

// ==================== Channel Subcommand Group ====================

#[poise::command(
    slash_command,
    rename = "channel",
    guild_only,
    subcommands("channel_set_filter", "channel_clear_filter", "channel_view"),
    subcommand_required,
    owners_only
)]
pub async fn channel(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Set filter for a channel
#[poise::command(slash_command, rename = "set-filter", guild_only, ephemeral, owners_only)]
pub async fn channel_set_filter(
    ctx: Context<'_>,
    #[description = "Discord channel"] channel: ChannelId,
    #[description = "Filter ID to assign"] filter_id: i64,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    // Check if filter exists
    let filter = db.get_notification_filter(filter_id).await?;
    if filter.is_none() {
        ctx.say(format!("❌ Filter with ID `{}` not found", filter_id))
            .await?;
        return Ok(());
    }

    // Check if channel exists
    let existing_channel = db.get_discord_channel(channel.get() as i64).await?;
    if existing_channel.is_none() {
        ctx.say(format!(
            "❌ Channel <#{}> is not registered in the database.\nPlease create it using `/avatar add` first or register it manually.",
            channel.get()
        ))
        .await?;
        return Ok(());
    }

    // Set filter
    db.update_channel_filter(channel.get() as i64, Some(filter_id))
        .await?;

    ctx.say(format!(
        "✅ Filter `{}` has been assigned to <#{}>\n\nThis channel will now use the specified filter for notifications.",
        filter_id,
        channel.get()
    ))
    .await?;

    Ok(())
}

/// Clear filter from a channel
#[poise::command(slash_command, rename = "clear-filter", guild_only, ephemeral, owners_only)]
pub async fn channel_clear_filter(
    ctx: Context<'_>,
    #[description = "Discord channel"] channel: ChannelId,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    // Check if channel exists
    let existing_channel = db.get_discord_channel(channel.get() as i64).await?;
    if existing_channel.is_none() {
        ctx.say(format!("❌ Channel <#{}> is not registered in the database.", channel.get()))
            .await?;
        return Ok(());
    }

    let old_filter_id = existing_channel.unwrap().filter_id;

    // Clear filter (set to None)
    db.update_channel_filter(channel.get() as i64, None).await?;

    let message = if let Some(fid) = old_filter_id {
        format!(
            "✅ Filter `{}` has been cleared from <#{}>\n\nThis channel will now use the guild's fallback channel routing.",
            fid,
            channel.get()
        )
    } else {
        format!(
            "ℹ️ Channel <#{}> had no filter assigned.",
            channel.get()
        )
    };

    ctx.say(message).await?;

    Ok(())
}

/// View channel filter information
#[poise::command(slash_command, rename = "view", guild_only, ephemeral, owners_only)]
pub async fn channel_view(
    ctx: Context<'_>,
    #[description = "Discord channel"] channel: ChannelId,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    // Get channel info
    let channel_info = db.get_discord_channel(channel.get() as i64).await?;

    match channel_info {
        Some(ch) => {
            let filter_info = if let Some(fid) = ch.filter_id {
                let filter = db.get_notification_filter(fid).await?;
                match filter {
                    Some(f) => {
                        format!(
                            "**Filter ID:** `{}`\n**Filter Preview:**\n```yaml\n{}\n```",
                            fid,
                            if f.rule_yaml.len() > 200 {
                                format!("{}...", &f.rule_yaml[..200])
                            } else {
                                f.rule_yaml
                            }
                        )
                    }
                    None => format!("⚠️ Filter ID `{}` (NOT FOUND - orphaned reference)", fid),
                }
            } else {
                "No filter assigned (using fallback routing)".to_string()
            };

            ctx.say(format!(
                "**🔍 Channel Information**\n\n**Channel:** <#{}>\n**Name:** `{}`\n**Created:** <t:{}:R>\n\n{}",
                ch.channel_id,
                ch.name,
                ch.created_at.unix_timestamp(),
                filter_info
            ))
            .await?;
        }
        None => {
            ctx.say(format!(
                "❌ Channel <#{}> is not registered in the database.",
                channel.get()
            ))
            .await?;
        }
    }

    Ok(())
}
