use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow};
use axum::{
    Router,
    extract::{Form, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rand::{Rng, distributions::Alphanumeric};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::{
    database::{DatabaseClient, DiscordGuild, NewDiscordChannel, NewNotificationFilter},
    filter::Filter,
};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const SESSION_COOKIE: &str = "bn_session";
const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7);
const OAUTH_STATE_TTL: Duration = Duration::from_secs(60 * 10);
const PERMISSION_ADMINISTRATOR: u64 = 0x8;
const PERMISSION_MANAGE_GUILD: u64 = 0x20;

#[derive(Clone)]
pub struct WebConfig {
    pub bind: SocketAddr,
    pub discord_client_id: String,
    pub discord_client_secret: String,
    pub discord_redirect_uri: String,
    pub bot_token: String,
    pub db: DatabaseClient,
    pub owner_ids: HashSet<u64>,
    pub cookie_secure: bool,
}

impl WebConfig {
    pub fn from_env(
        db: DatabaseClient,
        bot_token: String,
        owner_ids: HashSet<u64>,
    ) -> Result<Option<Self>> {
        let Some(bind) = optional_env("WEB_BIND") else {
            return Ok(None);
        };

        let bind = bind
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid WEB_BIND: {bind}"))?;
        let base_url = std::env::var("WEB_BASE_URL")
            .unwrap_or_else(|_| "https://booth-notifier.mii.dev".to_string());
        let discord_client_id = std::env::var("DISCORD_CLIENT_ID")?;
        let discord_client_secret = std::env::var("DISCORD_CLIENT_SECRET")?;
        let discord_redirect_uri = std::env::var("DISCORD_REDIRECT_URI")
            .unwrap_or_else(|_| format!("{base_url}/auth/discord/callback"));
        let cookie_secure = std::env::var("WEB_COOKIE_SECURE")
            .ok()
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or_else(|| base_url.starts_with("https://"));

        Ok(Some(Self {
            bind,
            discord_client_id,
            discord_client_secret,
            discord_redirect_uri,
            bot_token,
            db,
            owner_ids,
            cookie_secure,
        }))
    }
}

fn optional_env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

#[derive(Clone)]
struct AppState {
    db: DatabaseClient,
    http: reqwest::Client,
    discord_client_id: String,
    discord_client_secret: String,
    discord_redirect_uri: String,
    bot_token: String,
    owner_ids: HashSet<u64>,
    cookie_secure: bool,
    sessions: Arc<Mutex<HashMap<String, WebSession>>>,
    oauth_states: Arc<Mutex<HashMap<String, SystemTime>>>,
}

#[derive(Debug, Clone)]
struct WebSession {
    user: DiscordUser,
    guilds: Vec<OAuthGuild>,
    expires_at: SystemTime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DiscordUser {
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OAuthGuild {
    id: String,
    name: String,
    icon: Option<String>,
    owner: bool,
    permissions: String,
}

#[derive(Debug, Deserialize)]
struct OAuthCallback {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct DiscordApiChannel {
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: u8,
    #[serde(default)]
    nsfw: bool,
}

#[derive(Debug, Deserialize)]
struct FilterForm {
    rule_yaml: String,
}

#[derive(Debug, Deserialize)]
struct RegisterChannelForm {
    channel_id: String,
}

#[derive(Debug, Deserialize)]
struct SetFilterForm {
    filter_id: String,
}

#[derive(Debug, Deserialize)]
struct GuildSettingsForm {
    fallback_channel_id: String,
    fallback_nsfw_channel_id: String,
    general_category_id: String,
    nsfw_category_id: String,
}

#[derive(Debug)]
struct WebError(anyhow::Error);

impl<E> From<E> for WebError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        warn!(error = %self.0, "web request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(page("Error", None, &format!(
                r#"<section class="panel"><h1>Request failed</h1><p>{}</p><p><a href="/">Back</a></p></section>"#,
                escape(&self.0.to_string())
            ))),
        )
            .into_response()
    }
}

pub async fn serve(config: WebConfig) -> Result<()> {
    let bind = config.bind;
    let state = AppState {
        db: config.db,
        http: reqwest::Client::new(),
        discord_client_id: config.discord_client_id,
        discord_client_secret: config.discord_client_secret,
        discord_redirect_uri: config.discord_redirect_uri,
        bot_token: config.bot_token,
        owner_ids: config.owner_ids,
        cookie_secure: config.cookie_secure,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        oauth_states: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/", get(index))
        .route("/login", get(login))
        .route("/auth/discord/login", get(discord_login))
        .route("/auth/discord/callback", get(discord_callback))
        .route("/logout", post(logout))
        .route("/guilds/:guild_id", get(guild_home))
        .route(
            "/guilds/:guild_id/filters",
            get(filters_page).post(create_filter),
        )
        .route(
            "/guilds/:guild_id/filters/:filter_id",
            get(edit_filter_page).post(update_filter),
        )
        .route(
            "/guilds/:guild_id/filters/:filter_id/delete",
            post(delete_filter),
        )
        .route("/guilds/:guild_id/channels", get(channels_page))
        .route(
            "/guilds/:guild_id/channels/register",
            post(register_channel),
        )
        .route(
            "/guilds/:guild_id/channels/:channel_id/filter",
            post(set_channel_filter),
        )
        .route(
            "/guilds/:guild_id/channels/:channel_id/clear-filter",
            post(clear_channel_filter),
        )
        .route(
            "/guilds/:guild_id/channels/:channel_id/delete",
            post(delete_channel),
        )
        .route(
            "/guilds/:guild_id/settings",
            get(settings_page).post(update_settings),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, "web UI listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn login(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, WebError> {
    if current_session(&state, &headers).await?.is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    Ok(Html(page(
        "Login",
        None,
        r#"<section class="login"><h1>Booth Notifier</h1><p>Manage Discord server filters and notification channels.</p><a class="primary" href="/auth/discord/login">Login with Discord</a></section>"#,
    ))
    .into_response())
}

async fn discord_login(State(state): State<AppState>) -> Result<Response, WebError> {
    let oauth_state = random_token(32);
    state
        .oauth_states
        .lock()
        .await
        .insert(oauth_state.clone(), SystemTime::now() + OAUTH_STATE_TTL);

    let url = format!(
        "https://discord.com/oauth2/authorize?response_type=code&client_id={}&scope=identify%20guilds&redirect_uri={}&state={}",
        encode_component(&state.discord_client_id),
        encode_component(&state.discord_redirect_uri),
        encode_component(&oauth_state)
    );
    Ok(Redirect::to(&url).into_response())
}

async fn discord_callback(
    State(state): State<AppState>,
    Query(callback): Query<OAuthCallback>,
) -> Result<Response, WebError> {
    let valid_state = {
        let mut states = state.oauth_states.lock().await;
        states.retain(|_, expires_at| *expires_at > SystemTime::now());
        states.remove(&callback.state).is_some()
    };
    if !valid_state {
        return Ok((StatusCode::BAD_REQUEST, "invalid oauth state").into_response());
    }

    let token = state
        .http
        .post(format!("{DISCORD_API_BASE}/oauth2/token"))
        .form(&[
            ("client_id", state.discord_client_id.as_str()),
            ("client_secret", state.discord_client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", callback.code.as_str()),
            ("redirect_uri", state.discord_redirect_uri.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<DiscordTokenResponse>()
        .await?;

    let authorization = format!("{} {}", token.token_type, token.access_token);
    let user = state
        .http
        .get(format!("{DISCORD_API_BASE}/users/@me"))
        .header(header::AUTHORIZATION, &authorization)
        .send()
        .await?
        .error_for_status()?
        .json::<DiscordUser>()
        .await?;
    let guilds = state
        .http
        .get(format!("{DISCORD_API_BASE}/users/@me/guilds"))
        .header(header::AUTHORIZATION, &authorization)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<OAuthGuild>>()
        .await?;

    let session_id = random_token(48);
    let session = WebSession {
        user,
        guilds,
        expires_at: SystemTime::now() + SESSION_TTL,
    };
    state
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), session);

    let mut response = Redirect::to("/").into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(&state, &session_id))?,
    );
    Ok(response)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, WebError> {
    if let Some(session_id) = session_cookie_value(&headers) {
        state.sessions.lock().await.remove(&session_id);
    }
    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&expired_session_cookie(&state))?,
    );
    Ok(response)
}

async fn index(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, WebError> {
    let Some(session) = require_session(&state, &headers).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let guilds = manageable_guilds(&state, &session).await?;
    let mut rows = String::new();
    for guild in guilds {
        rows.push_str(&format!(
            r#"<a class="guild-row" href="/guilds/{id}"><span>{name}</span><small>{id}</small></a>"#,
            id = guild.guild_id,
            name = escape(&guild.name)
        ));
    }
    if rows.is_empty() {
        rows.push_str(r#"<div class="empty">No manageable registered servers were found.</div>"#);
    }
    Ok(Html(page(
        "Servers",
        Some(&session),
        &format!(r#"<section class="panel"><h1>Servers</h1><div class="guild-list">{rows}</div></section>"#),
    ))
    .into_response())
}

async fn guild_home(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
) -> Result<Response, WebError> {
    let Some((session, guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    Ok(Html(page(
        &guild.name,
        Some(&session),
        &format!(
            r#"<section class="panel"><div class="crumb"><a href="/">Servers</a> / {name}</div><h1>{name}</h1><nav class="actions"><a href="/guilds/{id}/filters">Filters</a><a href="/guilds/{id}/channels">Channels</a><a href="/guilds/{id}/settings">Settings</a></nav></section>"#,
            id = guild.guild_id,
            name = escape(&guild.name)
        ),
    ))
    .into_response())
}

async fn filters_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
) -> Result<Response, WebError> {
    let Some((session, guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let filters = state.db.get_notification_filters_by_guild(guild_id).await?;
    let channels = state.db.get_channels_by_guild(guild_id).await?;
    let mut list = String::new();
    for filter in filters {
        let linked = channels
            .iter()
            .filter(|c| c.filter_id == Some(filter.id))
            .count();
        list.push_str(&format!(
            r#"<article class="card"><div><h2>Filter #{id}</h2><p>{linked} linked channel(s)</p></div><pre>{yaml}</pre><div class="row-actions"><a href="/guilds/{guild_id}/filters/{id}">Edit</a><form method="post" action="/guilds/{guild_id}/filters/{id}/delete"><button class="danger" type="submit">Delete</button></form></div></article>"#,
            id = filter.id,
            linked = linked,
            guild_id = guild_id,
            yaml = escape(&filter.rule_yaml)
        ));
    }
    if list.is_empty() {
        list.push_str(r#"<div class="empty">No filters yet.</div>"#);
    }
    Ok(Html(page(
        "Filters",
        Some(&session),
        &format!(
            r#"<section class="panel"><div class="crumb"><a href="/guilds/{guild_id}">{guild_name}</a> / Filters</div><h1>Filters</h1><form id="new-filter-form" class="editor" method="post" action="/guilds/{guild_id}/filters">{builder}<details><summary>YAML source</summary><textarea name="rule_yaml" required rows="12">{sample}</textarea></details><button class="primary" type="submit">Create filter</button></form><div class="cards">{list}</div></section>"#,
            guild_id = guild_id,
            guild_name = escape(&guild.name),
            builder = filter_editor("new-filter-form", SAMPLE_FILTER)?,
            sample = escape(SAMPLE_FILTER),
            list = list
        ),
    ))
    .into_response())
}

async fn create_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
    Form(form): Form<FilterForm>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let rule_yaml = normalize_filter(&form.rule_yaml)?;
    state
        .db
        .create_notification_filter(NewNotificationFilter {
            guild_id: Some(guild_id),
            rule_yaml,
        })
        .await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/filters")).into_response())
}

async fn edit_filter_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((guild_id, filter_id)): Path<(i64, i64)>,
) -> Result<Response, WebError> {
    let Some((session, guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let Some(filter) = state.db.get_notification_filter(filter_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if filter.guild_id != Some(guild_id) && filter.guild_id.is_some() {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    Ok(Html(page(
        "Edit Filter",
        Some(&session),
        &format!(
            r#"<section class="panel"><div class="crumb"><a href="/guilds/{guild_id}">{guild_name}</a> / <a href="/guilds/{guild_id}/filters">Filters</a> / #{filter_id}</div><h1>Edit filter #{filter_id}</h1><form id="edit-filter-form" class="editor" method="post" action="/guilds/{guild_id}/filters/{filter_id}">{builder}<details><summary>YAML source</summary><textarea name="rule_yaml" required rows="18">{yaml}</textarea></details><button class="primary" type="submit">Save filter</button></form></section>"#,
            guild_id = guild_id,
            guild_name = escape(&guild.name),
            filter_id = filter_id,
            builder = filter_editor("edit-filter-form", &filter.rule_yaml)?,
            yaml = escape(&filter.rule_yaml)
        ),
    ))
    .into_response())
}

async fn update_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((guild_id, filter_id)): Path<(i64, i64)>,
    Form(form): Form<FilterForm>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let rule_yaml = normalize_filter(&form.rule_yaml)?;
    let updated = state
        .db
        .update_notification_filter(filter_id, guild_id, rule_yaml)
        .await?;
    if updated.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    Ok(Redirect::to(&format!("/guilds/{guild_id}/filters")).into_response())
}

async fn delete_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((guild_id, filter_id)): Path<(i64, i64)>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let Some(filter) = state.db.get_notification_filter(filter_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if filter.guild_id != Some(guild_id) && filter.guild_id.is_some() {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    let channels = state.db.get_channels_by_guild(guild_id).await?;
    if channels
        .iter()
        .any(|channel| channel.filter_id == Some(filter_id))
    {
        return Ok((
            StatusCode::CONFLICT,
            "filter is still assigned to a channel",
        )
            .into_response());
    }
    state.db.delete_notification_filter(filter_id).await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/filters")).into_response())
}

async fn channels_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
) -> Result<Response, WebError> {
    let Some((session, guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let registered = state.db.get_channels_by_guild(guild_id).await?;
    let filters = state.db.get_notification_filters_by_guild(guild_id).await?;
    let discord_channels = fetch_discord_channels(&state, guild_id)
        .await
        .unwrap_or_default();

    let mut register_options = String::new();
    for channel in discord_channels
        .iter()
        .filter(|c| c.kind == 0 || c.kind == 5)
    {
        register_options.push_str(&format!(
            r#"<option value="{id}">#{name}{nsfw}</option>"#,
            id = escape(&channel.id),
            name = escape(&channel.name),
            nsfw = if channel.nsfw { " (NSFW)" } else { "" }
        ));
    }

    let mut rows = String::new();
    for channel in registered {
        let mut filter_options = String::from(r#"<option value="">No filter</option>"#);
        for filter in &filters {
            let selected = if channel.filter_id == Some(filter.id) {
                " selected"
            } else {
                ""
            };
            filter_options.push_str(&format!(
                r#"<option value="{id}"{selected}>Filter #{id}</option>"#,
                id = filter.id,
                selected = selected
            ));
        }
        rows.push_str(&format!(
            r#"<tr><td>#{name}</td><td><code>{id}</code></td><td><form method="post" action="/guilds/{guild_id}/channels/{id}/filter" class="inline"><select name="filter_id">{filter_options}</select><button type="submit">Save</button></form></td><td><form method="post" action="/guilds/{guild_id}/channels/{id}/clear-filter" class="inline"><button type="submit">Clear</button></form><form method="post" action="/guilds/{guild_id}/channels/{id}/delete" class="inline"><button class="danger" type="submit">Remove</button></form></td></tr>"#,
            id = channel.channel_id,
            guild_id = guild_id,
            name = escape(&channel.name),
            filter_options = filter_options
        ));
    }
    if rows.is_empty() {
        rows.push_str(r#"<tr><td colspan="4" class="empty">No registered channels.</td></tr>"#);
    }

    Ok(Html(page(
        "Channels",
        Some(&session),
        &format!(
            r#"<section class="panel"><div class="crumb"><a href="/guilds/{guild_id}">{guild_name}</a> / Channels</div><h1>Channels</h1><form class="toolbar" method="post" action="/guilds/{guild_id}/channels/register"><select name="channel_id" required>{register_options}</select><button class="primary" type="submit">Register channel</button></form><table><thead><tr><th>Channel</th><th>ID</th><th>Filter</th><th></th></tr></thead><tbody>{rows}</tbody></table></section>"#,
            guild_id = guild_id,
            guild_name = escape(&guild.name),
            register_options = register_options,
            rows = rows
        ),
    ))
    .into_response())
}

async fn register_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
    Form(form): Form<RegisterChannelForm>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let channel_id = form
        .channel_id
        .parse::<i64>()
        .context("invalid channel id")?;
    let discord_channels = fetch_discord_channels(&state, guild_id).await?;
    let channel = discord_channels
        .into_iter()
        .find(|channel| channel.id == form.channel_id && (channel.kind == 0 || channel.kind == 5))
        .ok_or_else(|| anyhow!("channel not found or unsupported"))?;
    state
        .db
        .upsert_discord_channel(NewDiscordChannel {
            channel_id,
            guild_id,
            name: channel.name,
            filter_id: None,
        })
        .await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/channels")).into_response())
}

async fn set_channel_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    Form(form): Form<SetFilterForm>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let filter_id = parse_optional_i64(&form.filter_id, "filter_id")?;
    if let Some(filter_id) = filter_id {
        let filter = state.db.get_notification_filter(filter_id).await?;
        if !matches!(filter, Some(ref f) if f.guild_id == Some(guild_id) || f.guild_id.is_none()) {
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
    }
    let channel = state.db.get_discord_channel(channel_id).await?;
    if !matches!(channel, Some(ref c) if c.guild_id == guild_id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    state
        .db
        .update_channel_filter(channel_id, filter_id)
        .await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/channels")).into_response())
}

async fn clear_channel_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((guild_id, channel_id)): Path<(i64, i64)>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let channel = state.db.get_discord_channel(channel_id).await?;
    if !matches!(channel, Some(ref c) if c.guild_id == guild_id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    state.db.update_channel_filter(channel_id, None).await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/channels")).into_response())
}

async fn delete_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((guild_id, channel_id)): Path<(i64, i64)>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    state
        .db
        .delete_discord_channel(channel_id, guild_id)
        .await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/channels")).into_response())
}

async fn settings_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
) -> Result<Response, WebError> {
    let Some((session, guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let discord_channels = fetch_discord_channels(&state, guild_id)
        .await
        .unwrap_or_default();
    let channel_options = options_for_channels(&discord_channels, &[0, 5]);
    let category_options = options_for_channels(&discord_channels, &[4]);
    Ok(Html(page(
        "Settings",
        Some(&session),
        &format!(
            r#"<section class="panel"><div class="crumb"><a href="/guilds/{guild_id}">{guild_name}</a> / Settings</div><h1>Settings</h1><form class="settings" method="post" action="/guilds/{guild_id}/settings"><label>Fallback channel<select name="fallback_channel_id"><option value="">Unset</option>{fallback_options}</select></label><label>Fallback NSFW channel<select name="fallback_nsfw_channel_id"><option value="">Unset</option>{fallback_nsfw_options}</select></label><label>General category<select name="general_category_id"><option value="">Unset</option>{general_category_options}</select></label><label>NSFW category<select name="nsfw_category_id"><option value="">Unset</option>{nsfw_category_options}</select></label><button class="primary" type="submit">Save settings</button></form></section>"#,
            guild_id = guild_id,
            guild_name = escape(&guild.name),
            fallback_options = mark_selected(&channel_options, guild.fallback_channel_id),
            fallback_nsfw_options = mark_selected(&channel_options, guild.fallback_nsfw_channel_id),
            general_category_options = mark_selected(&category_options, guild.general_category_id),
            nsfw_category_options = mark_selected(&category_options, guild.nsfw_category_id),
        ),
    ))
    .into_response())
}

async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<i64>,
    Form(form): Form<GuildSettingsForm>,
) -> Result<Response, WebError> {
    let Some((_session, _guild)) = require_guild(&state, &headers, guild_id).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    state
        .db
        .update_guild_special_channels(
            guild_id,
            parse_optional_i64(&form.fallback_channel_id, "fallback_channel_id")?,
            parse_optional_i64(&form.fallback_nsfw_channel_id, "fallback_nsfw_channel_id")?,
            parse_optional_i64(&form.general_category_id, "general_category_id")?,
            parse_optional_i64(&form.nsfw_category_id, "nsfw_category_id")?,
        )
        .await?;
    Ok(Redirect::to(&format!("/guilds/{guild_id}/settings")).into_response())
}

async fn current_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<WebSession>, WebError> {
    let Some(session_id) = session_cookie_value(headers) else {
        return Ok(None);
    };
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, session| session.expires_at > SystemTime::now());
    Ok(sessions.get(&session_id).cloned())
}

async fn require_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<WebSession>, WebError> {
    current_session(state, headers).await
}

async fn require_guild(
    state: &AppState,
    headers: &HeaderMap,
    guild_id: i64,
) -> Result<Option<(WebSession, DiscordGuild)>, WebError> {
    let Some(session) = require_session(state, headers).await? else {
        return Ok(None);
    };
    if !can_manage_guild(state, &session, guild_id) {
        return Ok(None);
    }
    let Some(guild) = state.db.get_discord_guild(guild_id).await? else {
        return Ok(None);
    };
    Ok(Some((session, guild)))
}

async fn manageable_guilds(state: &AppState, session: &WebSession) -> Result<Vec<DiscordGuild>> {
    let guilds = state.db.get_all_discord_guilds().await?;
    Ok(guilds
        .into_iter()
        .filter(|guild| can_manage_guild(state, session, guild.guild_id))
        .collect())
}

fn can_manage_guild(state: &AppState, session: &WebSession, guild_id: i64) -> bool {
    let Ok(user_id) = session.user.id.parse::<u64>() else {
        return false;
    };
    if state.owner_ids.contains(&user_id) {
        return true;
    }
    session.guilds.iter().any(|guild| {
        guild.id.parse::<i64>().ok() == Some(guild_id)
            && (guild.owner || has_management_permission(&guild.permissions))
    })
}

fn has_management_permission(permissions: &str) -> bool {
    let Ok(permissions) = permissions.parse::<u64>() else {
        return false;
    };
    permissions & PERMISSION_ADMINISTRATOR != 0 || permissions & PERMISSION_MANAGE_GUILD != 0
}

async fn fetch_discord_channels(state: &AppState, guild_id: i64) -> Result<Vec<DiscordApiChannel>> {
    let channels = state
        .http
        .get(format!("{DISCORD_API_BASE}/guilds/{guild_id}/channels"))
        .header(header::AUTHORIZATION, format!("Bot {}", state.bot_token))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<DiscordApiChannel>>()
        .await?;
    Ok(channels)
}

fn filter_editor(form_id: &str, initial_yaml: &str) -> Result<String> {
    let filter = serde_yaml::from_str::<Filter>(initial_yaml).or_else(|yaml_error| {
        serde_json::from_str::<Filter>(initial_yaml).map_err(|_| yaml_error)
    })?;
    let filter_json = script_json(&serde_json::to_string(&filter)?);
    Ok(format!(
        r#"<div class="filter-builder" data-form="{form_id}">
<script type="application/json" class="filter-data">{filter_json}</script>
<div class="builder-head"><h2>Visual editor</h2><button type="button" data-action="add-group">Add AND group</button></div>
<div class="builder-groups"></div>
</div>"#,
        form_id = escape(form_id),
        filter_json = filter_json
    ))
}

fn script_json(value: &str) -> String {
    value
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

fn normalize_filter(input: &str) -> Result<String> {
    let filter = serde_yaml::from_str::<Filter>(input)
        .or_else(|yaml_error| serde_json::from_str::<Filter>(input).map_err(|_| yaml_error))?;
    if filter.groups.is_empty() {
        return Err(anyhow!("filter must have at least one group"));
    }
    if filter.groups.iter().any(|group| group.rules.is_empty()) {
        return Err(anyhow!("filter groups must have at least one rule"));
    }
    Ok(serde_yaml::to_string(&filter)?)
}

fn parse_optional_i64(value: &str, field: &str) -> Result<Option<i64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    value
        .trim()
        .parse::<i64>()
        .map(Some)
        .with_context(|| format!("invalid {field}"))
}

fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == SESSION_COOKIE).then(|| value.to_string())
    })
}

fn session_cookie(state: &AppState, session_id: &str) -> String {
    let secure = if state.cookie_secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}={session_id}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{secure}",
        SESSION_TTL.as_secs()
    )
}

fn expired_session_cookie(state: &AppState) -> String {
    let secure = if state.cookie_secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{secure}")
}

fn random_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn options_for_channels(channels: &[DiscordApiChannel], kinds: &[u8]) -> String {
    let mut options = String::new();
    for channel in channels
        .iter()
        .filter(|channel| kinds.contains(&channel.kind))
    {
        options.push_str(&format!(
            r#"<option value="{id}">{prefix}{name}</option>"#,
            id = escape(&channel.id),
            prefix = if channel.kind == 4 { "" } else { "#" },
            name = escape(&channel.name)
        ));
    }
    options
}

fn mark_selected(options: &str, selected: Option<i64>) -> String {
    let Some(selected) = selected else {
        return options.to_string();
    };
    options.replace(
        &format!("value=\"{selected}\""),
        &format!("value=\"{selected}\" selected"),
    )
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn display_name(session: &WebSession) -> String {
    session
        .user
        .global_name
        .clone()
        .unwrap_or_else(|| session.user.username.clone())
}

fn page(title: &str, session: Option<&WebSession>, body: &str) -> String {
    let user_nav = session.map_or_else(
        || String::new(),
        |session| {
            format!(
                r#"<form method="post" action="/logout"><span>{}</span><button type="submit">Logout</button></form>"#,
                escape(&display_name(session))
            )
        },
    );
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title><style>{css}</style></head><body><header><a class="brand" href="/">Booth Notifier</a><nav>{user_nav}</nav></header><main>{body}</main><script>{js}</script></body></html>"#,
        title = escape(title),
        css = CSS,
        user_nav = user_nav,
        body = body,
        js = JS
    )
}

const SAMPLE_FILTER: &str = r#"groups:
- rules:
  - field: tags
    op: include
    pattern:
      type: text
      value: VRChat
    case_sensitive: false
    tag_mode: any
schema_version: 1
"#;

const CSS: &str = r#"
:root{color-scheme:light;--bg:#f7f8fa;--panel:#fff;--text:#20242a;--muted:#687384;--line:#d8dee8;--accent:#1264a3;--danger:#b42318;--control:#eef2f7;--soft:#f9fbfd}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;font-size:15px;line-height:1.45}header{height:56px;display:flex;align-items:center;justify-content:space-between;padding:0 24px;border-bottom:1px solid var(--line);background:#fff;position:sticky;top:0;z-index:2}.brand{font-weight:700;color:var(--text);text-decoration:none}nav form{display:flex;align-items:center;gap:12px;color:var(--muted)}main{max-width:1120px;margin:0 auto;padding:32px 20px}.login{max-width:460px;margin:12vh auto;padding:40px 0}.login h1{font-size:34px;margin:0 0 12px}.login p{color:var(--muted);margin:0 0 24px}.panel{background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:24px}.panel h1{font-size:26px;margin:0 0 20px}.panel h2{font-size:17px;margin:0}.crumb{font-size:13px;color:var(--muted);margin-bottom:8px}.crumb a{color:var(--accent);text-decoration:none}.actions{display:flex;gap:10px;flex-wrap:wrap}.actions a,.primary,button,a.primary{appearance:none;border:1px solid var(--accent);background:var(--accent);color:#fff;text-decoration:none;border-radius:6px;padding:9px 12px;font:inherit;line-height:1.2;cursor:pointer}button{appearance:none;border:1px solid var(--line);background:var(--control);border-radius:6px;padding:8px 10px;font:inherit;cursor:pointer;color:var(--text)}button.danger{border-color:#f3b7b2;background:#fff1f0;color:var(--danger)}button.subtle{background:#fff;color:var(--text);border-color:var(--line)}.guild-list{display:grid;gap:8px}.guild-row{display:flex;justify-content:space-between;align-items:center;gap:16px;border:1px solid var(--line);border-radius:6px;padding:14px 16px;color:var(--text);text-decoration:none}.guild-row:hover{border-color:var(--accent)}.guild-row small{color:var(--muted)}.editor,.settings{display:grid;gap:12px;margin-bottom:24px}.editor textarea{width:100%;min-height:220px;resize:vertical;border:1px solid var(--line);border-radius:6px;padding:12px;font:14px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace}.settings label{display:grid;gap:6px;color:var(--muted)}select,input[type=text]{min-width:160px;max-width:100%;border:1px solid var(--line);border-radius:6px;background:#fff;padding:8px 10px;font:inherit}input[type=checkbox]{width:16px;height:16px}.toolbar{display:flex;gap:10px;flex-wrap:wrap;margin-bottom:18px}.cards{display:grid;gap:12px}.card{border:1px solid var(--line);border-radius:8px;padding:16px;display:grid;gap:12px}.card h2{font-size:18px;margin:0}.card p{margin:4px 0 0;color:var(--muted)}pre{margin:0;max-height:220px;overflow:auto;background:#101820;color:#eef6ff;border-radius:6px;padding:12px;font:13px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace}.row-actions{display:flex;gap:8px;align-items:center}.row-actions a{color:var(--accent);text-decoration:none;padding:8px 0}table{width:100%;border-collapse:collapse}th,td{text-align:left;border-bottom:1px solid var(--line);padding:11px 8px;vertical-align:middle}th{font-size:13px;color:var(--muted);font-weight:600}.inline{display:inline-flex;align-items:center;gap:8px;margin-right:8px}.empty{color:var(--muted);padding:18px 0}details{border:1px solid var(--line);border-radius:8px;background:#fff}summary{cursor:pointer;padding:10px 12px;color:var(--muted)}details textarea{border:0;border-top:1px solid var(--line);border-radius:0 0 8px 8px}.filter-builder{display:grid;gap:12px;border:1px solid var(--line);background:var(--soft);border-radius:8px;padding:14px}.builder-head,.group-head{display:flex;align-items:center;justify-content:space-between;gap:10px}.builder-groups{display:grid;gap:12px}.filter-group{border:1px solid var(--line);background:#fff;border-radius:8px;padding:12px;display:grid;gap:10px}.filter-group h3{font-size:14px;margin:0;color:var(--muted);font-weight:600}.rule-list{display:grid;gap:8px}.filter-rule{display:grid;grid-template-columns:minmax(120px,1fr) minmax(120px,1fr) minmax(120px,1fr) minmax(180px,2fr) auto auto;gap:8px;align-items:end;border:1px solid #e8edf4;background:#fff;border-radius:6px;padding:10px}.filter-rule label{display:grid;gap:4px;color:var(--muted);font-size:12px;min-width:0}.filter-rule select,.filter-rule input[type=text]{min-width:0;width:100%}.check-label{display:flex!important;align-items:center;gap:6px;min-height:38px}.tag-mode-wrap.is-hidden{visibility:hidden}.rule-footer{display:flex;justify-content:flex-start;grid-column:1/-1}@media(max-width:860px){.filter-rule{grid-template-columns:1fr 1fr}.filter-rule .rule-footer{justify-content:flex-start}}@media(max-width:720px){header{padding:0 14px}main{padding:18px 12px}.panel{padding:16px}.guild-row,td,th{display:block}.toolbar,.inline{display:flex;width:100%}select,.toolbar button,.inline button{width:100%}table,thead,tbody,tr{display:block}thead{display:none}tr{border-bottom:1px solid var(--line);padding:10px 0}td{border:0;padding:6px 0}.builder-head,.group-head{align-items:flex-start;flex-direction:column}.filter-rule{grid-template-columns:1fr}select,input[type=text]{width:100%}}
"#;

const JS: &str = r#"
(function(){
  const fields = ['tags','name','description','category'];
  const ops = ['include','exclude'];
  const patternTypes = ['text','regex'];
  const tagModes = ['any','all'];

  function option(value, selected){
    return `<option value="${value}"${value === selected ? ' selected' : ''}>${label(value)}</option>`;
  }
  function label(value){
    return value.split('_').map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(' ');
  }
  function defaultRule(){
    return {field:'tags',op:'include',pattern:{type:'text',value:''},case_sensitive:false,tag_mode:'any'};
  }
  function defaultFilter(){
    return {groups:[{rules:[defaultRule()]}],schema_version:1};
  }
  function normalizeFilter(filter){
    if (!filter || !Array.isArray(filter.groups) || filter.groups.length === 0) return defaultFilter();
    filter.schema_version = filter.schema_version || 1;
    filter.groups = filter.groups.map((group) => ({rules:(Array.isArray(group.rules) && group.rules.length ? group.rules : [defaultRule()]).map(normalizeRule)}));
    return filter;
  }
  function normalizeRule(rule){
    const next = Object.assign(defaultRule(), rule || {});
    next.pattern = Object.assign({type:'text',value:''}, next.pattern || {});
    if (!fields.includes(next.field)) next.field = 'tags';
    if (!ops.includes(next.op)) next.op = 'include';
    if (!patternTypes.includes(next.pattern.type)) next.pattern.type = 'text';
    if (next.field === 'tags' && !tagModes.includes(next.tag_mode)) next.tag_mode = 'any';
    return next;
  }
  function readRule(node){
    const field = node.querySelector('[data-name="field"]').value;
    const rule = {
      field,
      op: node.querySelector('[data-name="op"]').value,
      pattern: {
        type: node.querySelector('[data-name="pattern_type"]').value,
        value: node.querySelector('[data-name="pattern_value"]').value
      },
      case_sensitive: node.querySelector('[data-name="case_sensitive"]').checked
    };
    if (field === 'tags') rule.tag_mode = node.querySelector('[data-name="tag_mode"]').value;
    return rule;
  }
  function readFilter(builder){
    const groups = Array.from(builder.querySelectorAll('.filter-group')).map((group) => ({
      rules: Array.from(group.querySelectorAll('.filter-rule')).map(readRule)
    })).filter((group) => group.rules.length > 0);
    return {groups: groups.length ? groups : [{rules:[defaultRule()]}], schema_version: 1};
  }
  function yamlScalar(value){
    const text = String(value || '');
    if (/^(?:[-+]?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?|true|false|null|~)$/i.test(text)) return JSON.stringify(text);
    if (/^[A-Za-z0-9 _.,:+#@/-]+$/.test(text) && text.trim() === text && text !== '') return text;
    return JSON.stringify(text);
  }
  function toYaml(filter){
    let yaml = 'groups:\n';
    filter.groups.forEach((group) => {
      yaml += '- rules:\n';
      group.rules.forEach((rule) => {
        yaml += `  - field: ${rule.field}\n`;
        yaml += `    op: ${rule.op}\n`;
        yaml += '    pattern:\n';
        yaml += `      type: ${rule.pattern.type}\n`;
        yaml += `      value: ${yamlScalar(rule.pattern.value)}\n`;
        yaml += `    case_sensitive: ${rule.case_sensitive ? 'true' : 'false'}\n`;
        if (rule.field === 'tags') yaml += `    tag_mode: ${rule.tag_mode || 'any'}\n`;
      });
    });
    yaml += 'schema_version: 1\n';
    return yaml;
  }
  function syncYaml(builder){
    const form = document.getElementById(builder.dataset.form);
    if (!form) return;
    const textarea = form.querySelector('textarea[name="rule_yaml"]');
    if (textarea) textarea.value = toYaml(readFilter(builder));
  }
  function renderRule(rule, groupIndex, ruleIndex){
    rule = normalizeRule(rule);
    const tagHidden = rule.field === 'tags' ? '' : ' is-hidden';
    return `<div class="filter-rule" data-rule-index="${ruleIndex}">
      <label>Field<select data-name="field">${fields.map((value) => option(value, rule.field)).join('')}</select></label>
      <label>Operation<select data-name="op">${ops.map((value) => option(value, rule.op)).join('')}</select></label>
      <label>Match<select data-name="pattern_type">${patternTypes.map((value) => option(value, rule.pattern.type)).join('')}</select></label>
      <label>Value<input data-name="pattern_value" type="text" value="${escapeAttr(rule.pattern.value || '')}"></label>
      <label class="check-label"><input data-name="case_sensitive" type="checkbox"${rule.case_sensitive ? ' checked' : ''}> Case</label>
      <label class="tag-mode-wrap${tagHidden}">Tags<select data-name="tag_mode">${tagModes.map((value) => option(value, rule.tag_mode || 'any')).join('')}</select></label>
      <div class="rule-footer"><button type="button" class="danger" data-action="remove-rule">Remove</button></div>
    </div>`;
  }
  function render(builder, filter){
    filter = normalizeFilter(filter);
    const root = builder.querySelector('.builder-groups');
    root.innerHTML = filter.groups.map((group, groupIndex) => `
      <div class="filter-group" data-group-index="${groupIndex}">
        <div class="group-head"><h3>AND group ${groupIndex + 1}</h3><div class="row-actions"><button type="button" data-action="add-rule">Add OR rule</button><button type="button" class="danger" data-action="remove-group">Remove group</button></div></div>
        <div class="rule-list">${group.rules.map((rule, ruleIndex) => renderRule(rule, groupIndex, ruleIndex)).join('')}</div>
      </div>`).join('');
    syncYaml(builder);
  }
  function escapeAttr(value){
    return String(value).replace(/&/g,'&amp;').replace(/"/g,'&quot;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  }
  function attach(builder){
    let filter = normalizeFilter(JSON.parse(builder.querySelector('.filter-data').textContent));
    render(builder, filter);
    builder.addEventListener('click', (event) => {
      const button = event.target.closest('button[data-action]');
      if (!button) return;
      const action = button.dataset.action;
      filter = readFilter(builder);
      const groupNode = button.closest('.filter-group');
      const ruleNode = button.closest('.filter-rule');
      const groupIndex = groupNode ? Number(groupNode.dataset.groupIndex) : -1;
      const ruleIndex = ruleNode ? Number(ruleNode.dataset.ruleIndex) : -1;
      if (action === 'add-group') filter.groups.push({rules:[defaultRule()]});
      if (action === 'remove-group' && filter.groups.length > 1) filter.groups.splice(groupIndex, 1);
      if (action === 'add-rule' && groupIndex >= 0) filter.groups[groupIndex].rules.push(defaultRule());
      if (action === 'remove-rule' && groupIndex >= 0 && ruleIndex >= 0 && filter.groups[groupIndex].rules.length > 1) filter.groups[groupIndex].rules.splice(ruleIndex, 1);
      render(builder, filter);
    });
    builder.addEventListener('input', () => syncYaml(builder));
    builder.addEventListener('change', (event) => {
      const rule = event.target.closest('.filter-rule');
      if (rule && event.target.dataset.name === 'field') {
        rule.querySelector('.tag-mode-wrap').classList.toggle('is-hidden', event.target.value !== 'tags');
      }
      syncYaml(builder);
    });
    const form = document.getElementById(builder.dataset.form);
    if (form) form.addEventListener('submit', () => syncYaml(builder));
  }
  document.querySelectorAll('.filter-builder').forEach(attach);
})();
"#;
