use anyhow::Result;
use poise::serenity_prelude::{
    self as serenity, CacheHttp, ChannelId, CreateEmbed, CreateMessage, GuildChannel, GuildId,
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
            let channel_id = ChannelId::new(channel_id as u64);
            let message = self.create_message(item);
            self.send_message(ctx, channel_id, message).await?;
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
        let Some(filter_id) = channel.filter_id else {
            return Ok(false);
        };

        let filter: Filter = if let Some(filter) = db
            .get_notification_filter(filter_id)
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
            let message = self.create_message(item);
            self.send_message(ctx, channel_id, message).await?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn send_message(
        &self,
        ctx: &serenity::Context,
        channel: ChannelId,
        message: CreateMessage,
    ) -> Result<()> {
        let message = channel.send_message(&ctx.http(), message).await?;
        message.crosspost(&ctx.http()).await?;

        Ok(())
    }

    fn create_message(&self, item: &BoothItem) -> CreateMessage {
        CreateMessage::new().embed({
            let mut embed = CreateEmbed::new()
                .title(item.name.clone())
                .url(item.url.clone())
                .description(format!(
                    "{}\n価格: {}\nタグ: {}",
                    item.shop.name,
                    item.price,
                    item.tags
                        .iter()
                        .map(|tag| tag.name.clone())
                        .collect::<Vec<String>>()
                        .join(", ")
                ));

            if let Some(image) = item.images.first() {
                embed = embed.image(image.original.clone());
            }

            embed
        })
    }
}
