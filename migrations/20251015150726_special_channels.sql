ALTER TABLE discord_guilds
  ADD COLUMN fallback_channel_id bigint,
  ADD COLUMN fallback_nsfw_channel_id bigint,
  ADD COLUMN general_category_id bigint,
  ADD COLUMN nsfw_category_id bigint;

