use anyhow::Result;
use poise::serenity_prelude::{
    self as serenity, CacheHttp, ChannelId, CreateMessage, GuildChannel, GuildId,
};

use crate::{
    booth::item::BoothItem,
    database::{DatabaseClient, DiscordChannel, DiscordGuild},
    filter::{Filter, FilteringEngine},
};

pub struct NotifyTask;

impl NotifyTask {
    pub fn new() -> Self {
        Self
    }

    pub async fn notify(
        &self,
        ctx: &serenity::Context,
        db: &DatabaseClient,
        items: &[BoothItem],
    ) -> Result<()> {
        let guilds = db.get_all_discord_guilds().await?;

        for guild in &guilds {
            self.process_guild(ctx, db, guild, items).await?;
        }

        Ok(())
    }

    async fn process_guild(
        &self,
        ctx: &serenity::Context,
        db: &DatabaseClient,
        guild: &DiscordGuild,
        items: &[BoothItem],
    ) -> Result<()> {
        let channels = db.get_channels_by_guild(guild.guild_id).await?;

        for item in items {
            let mut notified = false;
            for channel in &channels {
                notified |= self.process_channel(ctx, db, channel, item).await?;
            }

            if notified {
                continue;
            }

            self.notify_to_fallback_channel(ctx, guild, item).await?;
        }

        Ok(())
    }

    async fn notify_to_fallback_channel(
        &self,
        ctx: &serenity::Context,
        guild: &DiscordGuild,
        item: &BoothItem,
    ) -> Result<()> {
        let is_nsfw = item.is_adult;
        let fallback_channel_id = if is_nsfw {
            guild.fallback_nsfw_channel_id
        } else {
            guild.fallback_channel_id
        };

        if let Some(channel_id) = fallback_channel_id {
            ChannelId::new(channel_id as u64)
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content(format!("{}\n{}", item.name, item.url)),
                )
                .await?;
        }

        Ok(())
    }

    async fn process_channel(
        &self,
        ctx: &serenity::Context,
        db: &DatabaseClient,
        channel: &DiscordChannel,
        item: &BoothItem,
    ) -> Result<bool> {
        if channel.filter_id.is_none() {
            return Ok(false);
        }

        let filter: Filter = if let Some(filter) = db
            .get_notification_filter(channel.filter_id.unwrap())
            .await?
        {
            if let Ok(filter) = serde_yaml::from_str(&filter.rule_yaml) {
                filter
            } else {
                return Ok(false);
            }
        } else {
            return Ok(false);
        };

        let engine = FilteringEngine::new(filter);

        let channel_id = ChannelId::new(channel.channel_id as u64);
        let is_nsfw_channel = match channel_id.to_channel(&ctx.http()).await? {
            serenity::Channel::Guild(channel) => channel.nsfw,
            _ => return Ok(false),
        };

        if !is_nsfw_channel && item.is_adult || is_nsfw_channel && !item.is_adult {
            return Ok(false);
        }

        if engine.check(item) {
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content(format!("{}\n{}", item.name, item.url)),
                )
                .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
