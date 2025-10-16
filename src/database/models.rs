use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;
use sqlx::types::JsonValue;

/// A record of a fetch run that stores which items were fetched
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FetchRun {
    pub id: i64,
    pub fetched_at: OffsetDateTime,
    pub item_ids: Vec<i64>,
}

/// A snapshot of an item at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ItemSnapshot {
    pub id: i64,
    pub fetched_at: OffsetDateTime,
    pub item_id: i64,
    pub name: String,
    pub payload: JsonValue,
}

/// A Discord guild (server)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DiscordGuild {
    pub guild_id: i64,
    pub name: String,
    pub created_at: OffsetDateTime,
    pub fallback_channel_id: Option<i64>,
    pub fallback_nsfw_channel_id: Option<i64>,
    pub general_category_id: Option<i64>,
    pub nsfw_category_id: Option<i64>,
}

/// A notification filter rule stored as YAML
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationFilter {
    pub id: i64,
    pub rule_yaml: String,
    pub created_at: OffsetDateTime,
}

/// A Discord channel that can receive notifications
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DiscordChannel {
    pub channel_id: i64,
    pub guild_id: i64,
    pub name: String,
    pub created_at: OffsetDateTime,
    pub filter_id: Option<i64>,
}

/// Input struct for creating a new fetch run
#[derive(Debug, Clone)]
pub struct NewFetchRun {
    pub item_ids: Vec<i64>,
}

/// Input struct for creating a new item snapshot
#[derive(Debug, Clone)]
pub struct NewItemSnapshot {
    pub item_id: i64,
    pub name: String,
    pub payload: JsonValue,
}

/// Input struct for creating a new Discord guild
#[derive(Debug, Clone)]
pub struct NewDiscordGuild {
    pub guild_id: i64,
    pub name: String,
    pub fallback_channel_id: Option<i64>,
    pub fallback_nsfw_channel_id: Option<i64>,
    pub general_category_id: Option<i64>,
    pub nsfw_category_id: Option<i64>,
}

/// Input struct for creating a new notification filter
#[derive(Debug, Clone)]
pub struct NewNotificationFilter {
    pub rule_yaml: String,
}

/// Input struct for creating a new Discord channel
#[derive(Debug, Clone)]
pub struct NewDiscordChannel {
    pub channel_id: i64,
    pub guild_id: i64,
    pub name: String,
    pub filter_id: Option<i64>,
}
