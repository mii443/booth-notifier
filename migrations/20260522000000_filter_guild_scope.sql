ALTER TABLE notification_filters
  ADD COLUMN guild_id bigint REFERENCES discord_guilds(guild_id) ON DELETE CASCADE;

UPDATE notification_filters nf
SET guild_id = c.guild_id
FROM discord_channels c
WHERE c.filter_id = nf.id
  AND nf.guild_id IS NULL;

CREATE INDEX idx_notification_filters_guild_id ON notification_filters (guild_id);
