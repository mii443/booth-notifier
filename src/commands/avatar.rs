use poise::serenity_prelude::{ChannelType, CreateChannel};

use crate::{
    database::{NewDiscordChannel, NewNotificationFilter},
    filter::{Field, Filter, FilterGroup, Op, Pattern, Rule, TagMode},
    Context, Error,
};

#[poise::command(
    slash_command,
    rename = "avatar",
    guild_only,
    subcommands("add", "channel_register"),
    subcommand_required,
    owners_only
)]
pub async fn avatar_command(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(
    slash_command,
    rename = "channel_register",
    guild_only,
    ephemeral,
    owners_only
)]
pub async fn channel_register(ctx: Context<'_>, channel_name: String) -> Result<(), Error> {
    let db = ctx.data().db.clone();

    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;
    let db_guild = db.get_discord_guild(guild_id.get() as i64).await?;

    let Some(db_guild) = db_guild else {
        ctx.say("This guild is not registered. Please register the guild first.")
            .await?;
        return Ok(());
    };

    let channel = guild_id
        .channels(&ctx.http())
        .await?
        .into_values()
        .find(|c| c.name == channel_name);

    let Some(channel) = channel else {
        ctx.say("Channel not found").await?;
        return Ok(());
    };

    db.upsert_discord_channel(NewDiscordChannel {
        channel_id: channel.id.get() as i64,
        guild_id: db_guild.guild_id,
        name: channel_name.clone(),
        filter_id: None,
    })
    .await?;

    ctx.reply("チャンネルを登録しました").await?;

    Ok(())
}

#[poise::command(slash_command, rename = "add", guild_only, ephemeral, owners_only)]
pub async fn add(
    ctx: Context<'_>,
    avatar_name: String,
    item_id: Option<u64>,
    channel_name: String,
    create_nsfw: bool,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let mut filter = Filter {
        groups: vec![
            FilterGroup {
                rules: vec![Rule {
                    field: Field::Tags,
                    op: Op::Include,
                    pattern: Pattern::Text { value: avatar_name },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: Some(TagMode::Any),
                }],
            },
            FilterGroup {
                rules: vec![Rule {
                    field: Field::Tags,
                    op: Op::Include,
                    pattern: Pattern::Text {
                        value: "VRChat".to_string(),
                    },
                    case_sensitive: false,
                    regex_flags: None,
                    tag_mode: Some(TagMode::Any),
                }],
            },
        ],
        ..Default::default()
    };

    if let Some(item_id) = item_id {
        filter.groups[0].rules.push(Rule {
            field: Field::Description,
            op: Op::Include,
            pattern: Pattern::Text {
                value: item_id.to_string(),
            },
            case_sensitive: false,
            regex_flags: None,
            tag_mode: None,
        });
    }

    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;
    let db_guild = db.get_discord_guild(guild_id.get() as i64).await?;

    let Some(db_guild) = db_guild else {
        ctx.say("This guild is not registered. Please register the guild first.")
            .await?;
        return Ok(());
    };

    let general_category = if let Some(id) = db_guild.general_category_id {
        id
    } else {
        ctx.say("General category is not set. Please set the general category first.")
            .await?;
        return Ok(());
    };

    let sfw_channel = guild_id
        .create_channel(
            &ctx.http(),
            CreateChannel::new(channel_name.clone())
                .category(general_category as u64)
                .kind(ChannelType::News),
        )
        .await?;

    let nsfw_channel = if create_nsfw {
        if let Some(id) = db_guild.nsfw_category_id {
            let nsfw_channel = guild_id
                .create_channel(
                    &ctx.http(),
                    CreateChannel::new(format!("{channel_name}-nsfw"))
                        .category(id as u64)
                        .kind(ChannelType::News)
                        .nsfw(true),
                )
                .await?;
            Some(nsfw_channel.id.get() as i64)
        } else {
            None
        }
    } else {
        None
    };

    let filter = db
        .create_notification_filter(NewNotificationFilter {
            rule_yaml: serde_yaml::to_string(&filter)?,
        })
        .await?;
    let filter_id = filter.id;

    db.upsert_discord_channel(NewDiscordChannel {
        channel_id: sfw_channel.id.get() as i64,
        guild_id: db_guild.guild_id,
        name: channel_name.clone(),
        filter_id: Some(filter_id),
    })
    .await?;

    if let Some(nsfw_channel_id) = nsfw_channel {
        db.upsert_discord_channel(NewDiscordChannel {
            channel_id: nsfw_channel_id,
            guild_id: db_guild.guild_id,
            name: format!("{channel_name}-nsfw"),
            filter_id: Some(filter_id),
        })
        .await?;
    }

    ctx.reply("通知チャンネルを作成しました").await?;

    Ok(())
}
