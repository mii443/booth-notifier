use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::types::time::OffsetDateTime;
use sqlx::types::JsonValue;
use std::collections::HashMap;

use super::models::{
    DiscordChannel, DiscordGuild, FetchRun, ItemSnapshot, NewDiscordChannel, NewDiscordGuild,
    NewFetchRun, NewItemSnapshot, NewNotificationFilter, NotificationFilter,
};

/// Database client wrapper around sqlx::PgPool
#[derive(Clone)]
pub struct DatabaseClient {
    pool: PgPool,
}

impl DatabaseClient {
    /// Create a new database client from a database URL
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Get a reference to the underlying connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Create a new fetch run
    pub async fn create_fetch_run(&self, new_fetch_run: NewFetchRun) -> Result<FetchRun> {
        let fetch_run = sqlx::query_as!(
            FetchRun,
            r#"
            INSERT INTO fetch_runs (item_ids)
            VALUES ($1)
            RETURNING id, fetched_at, item_ids
            "#,
            &new_fetch_run.item_ids
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(fetch_run)
    }

    /// Get a fetch run by ID
    pub async fn get_fetch_run(&self, id: i64) -> Result<Option<FetchRun>> {
        let fetch_run = sqlx::query_as!(
            FetchRun,
            r#"
            SELECT id, fetched_at, item_ids
            FROM fetch_runs
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(fetch_run)
    }

    /// Get the latest fetch runs, limited by count
    pub async fn get_latest_fetch_runs(&self, limit: i64) -> Result<Vec<FetchRun>> {
        let fetch_runs = sqlx::query_as!(
            FetchRun,
            r#"
            SELECT id, fetched_at, item_ids
            FROM fetch_runs
            ORDER BY fetched_at DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(fetch_runs)
    }

    /// Create a new item snapshot
    pub async fn create_item_snapshot(
        &self,
        new_snapshot: NewItemSnapshot,
    ) -> Result<ItemSnapshot> {
        let snapshot = sqlx::query_as!(
            ItemSnapshot,
            r#"
            INSERT INTO item_snapshots (item_id, name, payload)
            VALUES ($1, $2, $3)
            RETURNING id, fetched_at, item_id, name, payload
            "#,
            new_snapshot.item_id,
            new_snapshot.name,
            new_snapshot.payload
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(snapshot)
    }

    /// Get all snapshots for a specific item
    pub async fn get_item_snapshots(&self, item_id: i64) -> Result<Vec<ItemSnapshot>> {
        let snapshots = sqlx::query_as!(
            ItemSnapshot,
            r#"
            SELECT id, fetched_at, item_id, name, payload
            FROM item_snapshots
            WHERE item_id = $1
            ORDER BY fetched_at DESC
            "#,
            item_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(snapshots)
    }

    /// Get the latest snapshot for a specific item
    pub async fn get_latest_snapshot(&self, item_id: i64) -> Result<Option<ItemSnapshot>> {
        let snapshot = sqlx::query_as!(
            ItemSnapshot,
            r#"
            SELECT id, fetched_at, item_id, name, payload
            FROM item_snapshots
            WHERE item_id = $1
            ORDER BY fetched_at DESC
            LIMIT 1
            "#,
            item_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(snapshot)
    }

    /// Get snapshots within a time range
    pub async fn get_snapshots_by_time_range(
        &self,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<Vec<ItemSnapshot>> {
        let snapshots = sqlx::query_as!(
            ItemSnapshot,
            r#"
            SELECT id, fetched_at, item_id, name, payload
            FROM item_snapshots
            WHERE fetched_at BETWEEN $1 AND $2
            ORDER BY fetched_at DESC
            "#,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(snapshots)
    }

    /// Insert or update a Discord guild
    pub async fn upsert_discord_guild(&self, new_guild: NewDiscordGuild) -> Result<DiscordGuild> {
        let guild = sqlx::query_as!(
            DiscordGuild,
            r#"
            INSERT INTO discord_guilds (guild_id, name, fallback_channel_id, fallback_nsfw_channel_id, general_category_id, nsfw_category_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (guild_id)
            DO UPDATE SET
                name = EXCLUDED.name,
                fallback_channel_id = EXCLUDED.fallback_channel_id,
                fallback_nsfw_channel_id = EXCLUDED.fallback_nsfw_channel_id,
                general_category_id = EXCLUDED.general_category_id,
                nsfw_category_id = EXCLUDED.nsfw_category_id
            RETURNING guild_id, name, created_at, fallback_channel_id, fallback_nsfw_channel_id, general_category_id, nsfw_category_id
            "#,
            new_guild.guild_id,
            new_guild.name,
            new_guild.fallback_channel_id,
            new_guild.fallback_nsfw_channel_id,
            new_guild.general_category_id,
            new_guild.nsfw_category_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(guild)
    }

    /// Get a Discord guild by ID
    pub async fn get_discord_guild(&self, guild_id: i64) -> Result<Option<DiscordGuild>> {
        let guild = sqlx::query_as!(
            DiscordGuild,
            r#"
            SELECT guild_id, name, created_at, fallback_channel_id, fallback_nsfw_channel_id, general_category_id, nsfw_category_id
            FROM discord_guilds
            WHERE guild_id = $1
            "#,
            guild_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(guild)
    }

    /// Get all Discord guilds
    pub async fn get_all_discord_guilds(&self) -> Result<Vec<DiscordGuild>> {
        let guilds = sqlx::query_as!(
            DiscordGuild,
            r#"
            SELECT guild_id, name, created_at, fallback_channel_id, fallback_nsfw_channel_id, general_category_id, nsfw_category_id
            FROM discord_guilds
            ORDER BY name
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(guilds)
    }

    /// Insert or update a Discord channel
    pub async fn upsert_discord_channel(
        &self,
        new_channel: NewDiscordChannel,
    ) -> Result<DiscordChannel> {
        let channel = sqlx::query_as!(
            DiscordChannel,
            r#"
            INSERT INTO discord_channels (channel_id, guild_id, name, filter_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (channel_id)
            DO UPDATE SET
                guild_id = EXCLUDED.guild_id,
                name = EXCLUDED.name,
                filter_id = EXCLUDED.filter_id
            RETURNING channel_id, guild_id, name, created_at, filter_id
            "#,
            new_channel.channel_id,
            new_channel.guild_id,
            new_channel.name,
            new_channel.filter_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(channel)
    }

    /// Get a Discord channel by ID
    pub async fn get_discord_channel(&self, channel_id: i64) -> Result<Option<DiscordChannel>> {
        let channel = sqlx::query_as!(
            DiscordChannel,
            r#"
            SELECT channel_id, guild_id, name, created_at, filter_id
            FROM discord_channels
            WHERE channel_id = $1
            "#,
            channel_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(channel)
    }

    /// Get all channels for a specific guild
    pub async fn get_channels_by_guild(&self, guild_id: i64) -> Result<Vec<DiscordChannel>> {
        let channels = sqlx::query_as!(
            DiscordChannel,
            r#"
            SELECT channel_id, guild_id, name, created_at, filter_id
            FROM discord_channels
            WHERE guild_id = $1
            ORDER BY name
            "#,
            guild_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(channels)
    }

    /// Create a new notification filter
    pub async fn create_notification_filter(
        &self,
        new_filter: NewNotificationFilter,
    ) -> Result<NotificationFilter> {
        let filter = sqlx::query_as!(
            NotificationFilter,
            r#"
            INSERT INTO notification_filters (rule_yaml)
            VALUES ($1)
            RETURNING id, rule_yaml, created_at
            "#,
            new_filter.rule_yaml
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(filter)
    }

    /// Get a notification filter by ID
    pub async fn get_notification_filter(&self, id: i64) -> Result<Option<NotificationFilter>> {
        let filter = sqlx::query_as!(
            NotificationFilter,
            r#"
            SELECT id, rule_yaml, created_at
            FROM notification_filters
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(filter)
    }

    /// Update the filter for a Discord channel
    pub async fn update_channel_filter(
        &self,
        channel_id: i64,
        filter_id: Option<i64>,
    ) -> Result<DiscordChannel> {
        let channel = sqlx::query_as!(
            DiscordChannel,
            r#"
            UPDATE discord_channels
            SET filter_id = $2
            WHERE channel_id = $1
            RETURNING channel_id, guild_id, name, created_at, filter_id
            "#,
            channel_id,
            filter_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(channel)
    }

    /// Delete a notification filter
    pub async fn delete_notification_filter(&self, id: i64) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            DELETE FROM notification_filters
            WHERE id = $1
            "#,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get all notification filters
    pub async fn get_all_notification_filters(&self) -> Result<Vec<NotificationFilter>> {
        let filters = sqlx::query_as!(
            NotificationFilter,
            r#"
            SELECT id, rule_yaml, created_at
            FROM notification_filters
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(filters)
    }

    /// Get multiple notification filters by IDs in a single query
    /// Returns a HashMap mapping filter ID to NotificationFilter for efficient lookups
    pub async fn get_notification_filters_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<HashMap<i64, NotificationFilter>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let filters = sqlx::query_as!(
            NotificationFilter,
            r#"
            SELECT id, rule_yaml, created_at
            FROM notification_filters
            WHERE id = ANY($1)
            "#,
            ids
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(filters.into_iter().map(|f| (f.id, f)).collect())
    }

    /// Update special channels configuration for a Discord guild
    pub async fn update_guild_special_channels(
        &self,
        guild_id: i64,
        fallback_channel_id: Option<i64>,
        fallback_nsfw_channel_id: Option<i64>,
        general_category_id: Option<i64>,
        nsfw_category_id: Option<i64>,
    ) -> Result<DiscordGuild> {
        let guild = sqlx::query_as!(
            DiscordGuild,
            r#"
            UPDATE discord_guilds
            SET
                fallback_channel_id = $2,
                fallback_nsfw_channel_id = $3,
                general_category_id = $4,
                nsfw_category_id = $5
            WHERE guild_id = $1
            RETURNING guild_id, name, created_at, fallback_channel_id, fallback_nsfw_channel_id, general_category_id, nsfw_category_id
            "#,
            guild_id,
            fallback_channel_id,
            fallback_nsfw_channel_id,
            general_category_id,
            nsfw_category_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(guild)
    }
}
