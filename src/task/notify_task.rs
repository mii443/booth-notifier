use anyhow::Result;
use poise::serenity_prelude::{self as serenity, CacheHttp, ChannelId, CreateEmbed, CreateMessage};
use std::collections::HashMap;
use tracing::info;

use crate::{
    booth::item::BoothItem,
    database::{models::NotificationFilter, DatabaseClient, DiscordChannel, DiscordGuild},
    filter::{Filter, FilteringEngine},
};

pub struct NotifyTask {
    nsfw_cache: HashMap<i64, bool>,
}

impl NotifyTask {
    pub fn new() -> Self {
        Self {
            nsfw_cache: HashMap::new(),
        }
    }

    pub async fn notify(
        &mut self,
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
        &mut self,
        ctx: &serenity::Context,
        db: &DatabaseClient,
        guild: &DiscordGuild,
        items: &[BoothItem],
    ) -> Result<()> {
        let channels = db.get_channels_by_guild(guild.guild_id).await?;

        let filter_ids: Vec<i64> = channels.iter().filter_map(|c| c.filter_id).collect();
        let filters = db.get_notification_filters_by_ids(&filter_ids).await?;

        for item in items {
            info!(
                "Processing item '{}' for guild '{}'",
                item.name, guild.guild_id
            );
            let mut notified = false;
            for channel in &channels {
                notified |= self.process_channel(ctx, channel, item, &filters).await?;
            }

            if notified {
                continue;
            }

            info!(
                "No channels matched for item '{}' in guild '{}', sending to fallback channel",
                item.name, guild.guild_id
            );
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
        &mut self,
        ctx: &serenity::Context,
        channel: &DiscordChannel,
        item: &BoothItem,
        filters: &HashMap<i64, NotificationFilter>,
    ) -> Result<bool> {
        let Some(filter_id) = channel.filter_id else {
            return Ok(false);
        };

        let filter: Filter = if let Some(filter) = filters.get(&filter_id) {
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
        let is_nsfw_channel = self.is_nsfw_channel(ctx, channel.channel_id).await?;

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

    async fn is_nsfw_channel(&mut self, ctx: &serenity::Context, channel_id: i64) -> Result<bool> {
        if let Some(is_nsfw) = self.nsfw_cache.get(&channel_id) {
            return Ok(*is_nsfw);
        }

        let channel = ChannelId::new(channel_id as u64)
            .to_channel(&ctx.http())
            .await?;

        let is_nsfw = match channel {
            serenity::Channel::Guild(channel) => channel.nsfw,
            _ => false,
        };

        self.nsfw_cache.insert(channel_id, is_nsfw);

        Ok(is_nsfw)
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
                    self.get_tags_str(item)
                ));

            if let Some(image) = item.images.first() {
                embed = embed.image(image.original.clone());
            }

            embed
        })
    }

    fn get_tags_str(&self, item: &BoothItem) -> String {
        let tags = item
            .tags
            .iter()
            .map(|tag| tag.name.clone())
            .collect::<Vec<String>>()
            .join(", ");

        if tags.len() <= 100 {
            tags
        } else {
            tags.chars().take(100).collect::<String>().to_string() + "..."
        }
    }
}
