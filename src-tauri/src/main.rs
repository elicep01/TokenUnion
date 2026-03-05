mod db;
mod p2p;
mod proxy;
mod tracker;
mod vault;

use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use db::{
    AuditLogRecord, DashboardStats, Db, FairUseRecord, PeerRecord, PoolStatusRecord, RateLimitStat,
    ScheduleConfig, SecurityEventRecord, TokenMessageRecord, TransactionRecord, VaultKeyDto, OAuthSessionRecord,
};
use p2p::{LocalNodeDto, P2pHandle};
use rand::RngCore;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tauri::{
    menu::MenuEvent,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, Wry,
};
use tauri_plugin_autostart::ManagerExt as AutoStartManagerExt;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::{oneshot, Mutex, RwLock};

struct ProxyRuntime {
    running: bool,
    port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

#[derive(Clone)]
struct InnerState {
    db: Arc<Db>,
    http: Client,
    vault_password: Arc<RwLock<Option<String>>>,
    proxy_runtime: Arc<Mutex<ProxyRuntime>>,
    p2p_handle: Arc<RwLock<Option<P2pHandle>>>,
}

type AppState = InnerState;

#[derive(serde::Serialize)]
struct AppPreferences {
    auto_start: bool,
    notifications_enabled: bool,
    appearance: String,
    proxy_port: u16,
}

#[derive(serde::Serialize)]
struct OAuthAuthorizePayload {
    authorize_url: String,
    state: String,
    code_verifier: String,
}

#[derive(serde::Serialize)]
struct AnthropicOAuthConfig {
    client_id: String,
    authorize_url: String,
    token_url: String,
    redirect_uri: String,
    scopes: String,
}

#[derive(serde::Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[tauri::command]
async fn unlock_vault(state: State<'_, AppState>, password: String) -> Result<(), String> {
    *state.vault_password.write().await = Some(password);
    Ok(())
}

#[tauri::command]
async fn set_provider_key(
    state: State<'_, AppState>,
    provider: String,
    label: String,
    api_key: String,
    password: String,
) -> Result<(), String> {
    let device_salt = state.db.get_or_create_device_salt().map_err(|e| e.to_string())?;
    let encrypted =
        vault::encrypt_api_key(&api_key, &password, &device_salt).map_err(|e| e.to_string())?;
    state
        .db
        .set_provider_key(&provider, &label, &encrypted)
        .map_err(|e| e.to_string())?;

    *state.vault_password.write().await = Some(password);
    Ok(())
}

#[tauri::command]
async fn create_anthropic_oauth_authorize_url(
    client_id: String,
    redirect_uri: String,
    scopes: String,
    authorize_base_url: Option<String>,
) -> Result<OAuthAuthorizePayload, String> {
    let mut verifier_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let challenge = {
        let digest = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    };
    let mut state_bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut state_bytes);
    let state = URL_SAFE_NO_PAD.encode(state_bytes);
    let auth_base = authorize_base_url.unwrap_or_else(|| "https://claude.ai/oauth/authorize".to_string());
    let scope_enc = urlencoding::encode(&scopes);
    let redirect_enc = urlencoding::encode(&redirect_uri);
    let authorize_url = format!(
        "{auth_base}?code=true&client_id={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}&redirect_uri={}",
        urlencoding::encode(&client_id),
        scope_enc,
        challenge,
        state,
        redirect_enc
    );
    Ok(OAuthAuthorizePayload {
        authorize_url,
        state,
        code_verifier,
    })
}

#[tauri::command]
async fn exchange_anthropic_oauth_code(
    state: State<'_, AppState>,
    token_url: String,
    client_id: String,
    redirect_uri: String,
    code: String,
    code_verifier: String,
    expected_state: String,
    returned_state: String,
    account_label: Option<String>,
) -> Result<(), String> {
    if expected_state != returned_state {
        return Err("OAuth state mismatch".to_string());
    }
    let vault_password = state
        .vault_password
        .read()
        .await
        .clone()
        .ok_or_else(|| "Vault is locked. Unlock vault before OAuth connect.".to_string())?;
    let device_salt = state.db.get_or_create_device_salt().map_err(|e| e.to_string())?;

    let resp = state
        .http
        .post(token_url)
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": client_id,
            "redirect_uri": redirect_uri,
            "code": code,
            "code_verifier": code_verifier
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_else(|_| "".to_string());
        return Err(format!("OAuth token exchange failed: {}", body));
    }
    let token_payload: OAuthTokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    let encrypted_access = vault::encrypt_api_key(&token_payload.access_token, &vault_password, &device_salt)
        .map_err(|e| e.to_string())?;
    let encrypted_refresh = match token_payload.refresh_token.as_deref() {
        Some(refresh) => Some(
            vault::encrypt_api_key(refresh, &vault_password, &device_salt)
                .map_err(|e| e.to_string())?,
        ),
        None => None,
    };
    let expires_at = token_payload
        .expires_in
        .map(|secs| (chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339());
    state
        .db
        .upsert_oauth_session(
            "anthropic",
            &encrypted_access,
            encrypted_refresh.as_deref(),
            expires_at.as_deref(),
            token_payload.scope.as_deref(),
            account_label.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn list_oauth_sessions(state: State<'_, AppState>) -> Result<Vec<OAuthSessionRecord>, String> {
    state.db.list_oauth_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_oauth_session(state: State<'_, AppState>, provider: String) -> Result<(), String> {
    state.db.delete_oauth_session(&provider).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_anthropic_oauth_config(state: State<'_, AppState>) -> Result<AnthropicOAuthConfig, String> {
    Ok(AnthropicOAuthConfig {
        client_id: state
            .db
            .get_setting("anthropic_oauth_client_id")
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
        authorize_url: state
            .db
            .get_setting("anthropic_oauth_authorize_url")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "https://claude.ai/oauth/authorize".to_string()),
        token_url: state
            .db
            .get_setting("anthropic_oauth_token_url")
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
        redirect_uri: state
            .db
            .get_setting("anthropic_oauth_redirect_uri")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "http://127.0.0.1:8787/oauth/callback".to_string()),
        scopes: state
            .db
            .get_setting("anthropic_oauth_scopes")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "user:profile user:inference user:sessions:claude_code user:mcp_servers".to_string()),
    })
}

#[tauri::command]
async fn set_anthropic_oauth_config(
    state: State<'_, AppState>,
    client_id: String,
    authorize_url: String,
    token_url: String,
    redirect_uri: String,
    scopes: String,
) -> Result<(), String> {
    state
        .db
        .set_setting("anthropic_oauth_client_id", &client_id)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("anthropic_oauth_authorize_url", &authorize_url)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("anthropic_oauth_token_url", &token_url)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("anthropic_oauth_redirect_uri", &redirect_uri)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("anthropic_oauth_scopes", &scopes)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn set_anthropic_key(
    state: State<'_, AppState>,
    label: String,
    api_key: String,
    password: String,
) -> Result<(), String> {
    set_provider_key(state, "anthropic".to_string(), label, api_key, password).await
}

#[tauri::command]
async fn list_keys(state: State<'_, AppState>) -> Result<Vec<VaultKeyDto>, String> {
    state.db.list_keys().map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_key(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_key(id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_proxy_port(state: State<'_, AppState>, port: u16) -> Result<(), String> {
    state
        .db
        .set_setting("proxy_port", &port.to_string())
        .map_err(|e| e.to_string())?;
    let mut runtime = state.proxy_runtime.lock().await;
    runtime.port = port;
    Ok(())
}

#[tauri::command]
async fn get_proxy_port(state: State<'_, AppState>) -> Result<u16, String> {
    read_proxy_port(&state.db).map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_proxy(state: State<'_, AppState>) -> Result<(), String> {
    start_proxy_inner(state.inner().clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_proxy(state: State<'_, AppState>) -> Result<(), String> {
    let mut runtime = state.proxy_runtime.lock().await;
    if let Some(tx) = runtime.shutdown_tx.take() {
        let _ = tx.send(());
    }
    runtime.running = false;
    Ok(())
}

#[tauri::command]
async fn get_dashboard_stats(state: State<'_, AppState>) -> Result<DashboardStats, String> {
    let (tokens, requests) = state.db.today_stats().map_err(|e| e.to_string())?;
    let label = state.db.active_key_label().map_err(|e| e.to_string())?;
    let runtime = state.proxy_runtime.lock().await;

    Ok(DashboardStats {
        total_tokens_today: tokens,
        requests_today: requests,
        active_key_label: label,
        proxy_running: runtime.running,
        proxy_port: runtime.port,
    })
}

#[tauri::command]
async fn get_local_node(state: State<'_, AppState>) -> Result<LocalNodeDto, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.local_node().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_profile(
    state: State<'_, AppState>,
    display_name: String,
    timezone: String,
) -> Result<(), String> {
    let handle = get_p2p_handle(&state).await?;
    handle
        .set_profile(display_name, timezone)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_invite_url(state: State<'_, AppState>) -> Result<String, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.create_invite_url().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn join_invite(state: State<'_, AppState>, invite_url: String) -> Result<(), String> {
    let handle = get_p2p_handle(&state).await?;
    handle
        .join_invite(invite_url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_peers(state: State<'_, AppState>) -> Result<Vec<PeerRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.list_peers().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_token_messages(state: State<'_, AppState>) -> Result<Vec<TokenMessageRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.list_messages().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_schedule_config(state: State<'_, AppState>) -> Result<ScheduleConfig, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.get_schedule_config().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_schedule_config(
    state: State<'_, AppState>,
    timezone: String,
    weekly_active_bitmap: String,
    sharing_override: String,
) -> Result<(), String> {
    let handle = get_p2p_handle(&state).await?;
    handle
        .set_schedule_config(timezone, weekly_active_bitmap, sharing_override)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn evaluate_schedule_tick(state: State<'_, AppState>) -> Result<String, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.evaluate_schedule_tick().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_pool_status(state: State<'_, AppState>) -> Result<Vec<PoolStatusRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.pool_status().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_ledger_transactions(state: State<'_, AppState>) -> Result<Vec<TransactionRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.list_transactions().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_fair_use_7d(state: State<'_, AppState>) -> Result<Vec<FairUseRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.fair_use_7d().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_audit_log(state: State<'_, AppState>) -> Result<Vec<AuditLogRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.list_audit_log().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_security_events(state: State<'_, AppState>) -> Result<Vec<SecurityEventRecord>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.list_security_events().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_rate_limit_stats(state: State<'_, AppState>) -> Result<Vec<RateLimitStat>, String> {
    let handle = get_p2p_handle(&state).await?;
    handle.list_rate_limit_stats().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_peer_rate_limit(
    state: State<'_, AppState>,
    peer_id: String,
    max_per_min: i64,
) -> Result<(), String> {
    let handle = get_p2p_handle(&state).await?;
    handle
        .set_peer_rate_limit(peer_id, max_per_min)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn leave_circle(state: State<'_, AppState>) -> Result<(), String> {
    let handle = get_p2p_handle(&state).await?;
    handle.leave_circle().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_content_filter_patterns(
    state: State<'_, AppState>,
    blocked_model_patterns: String,
) -> Result<(), String> {
    state
        .db
        .set_setting("blocked_model_patterns", &blocked_model_patterns)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_relay_mode(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state
        .db
        .get_setting("relay_mode")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "self_hosted".to_string()))
}

#[tauri::command]
async fn set_relay_mode(state: State<'_, AppState>, relay_mode: String) -> Result<(), String> {
    let normalized = match relay_mode.as_str() {
        "off" | "self_hosted" | "community" => relay_mode,
        _ => return Err("relay_mode must be one of: off|self_hosted|community".to_string()),
    };
    state
        .db
        .set_setting("relay_mode", &normalized)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_onboarding_completed(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state
        .db
        .get_setting("onboarding_completed")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "0".to_string())
        == "1")
}

#[tauri::command]
async fn complete_onboarding(state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .set_setting("onboarding_completed", "1")
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_app_preferences(state: State<'_, AppState>) -> Result<AppPreferences, String> {
    let auto_start = state
        .db
        .get_setting("pref_auto_start")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "0".to_string())
        == "1";
    let notifications_enabled = state
        .db
        .get_setting("pref_notifications")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "1".to_string())
        == "1";
    let appearance = state
        .db
        .get_setting("pref_appearance")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "dark".to_string());
    let proxy_port = read_proxy_port(&state.db).map_err(|e| e.to_string())?;

    Ok(AppPreferences {
        auto_start,
        notifications_enabled,
        appearance,
        proxy_port,
    })
}

#[tauri::command]
async fn set_app_preferences(
    app: AppHandle,
    state: State<'_, AppState>,
    auto_start: bool,
    notifications_enabled: bool,
    appearance: String,
) -> Result<(), String> {
    state
        .db
        .set_setting("pref_auto_start", if auto_start { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "pref_notifications",
            if notifications_enabled { "1" } else { "0" },
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("pref_appearance", &appearance)
        .map_err(|e| e.to_string())?;
    if auto_start {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn export_ledger_csv(state: State<'_, AppState>) -> Result<String, String> {
    let handle = get_p2p_handle(&state).await?;
    let rows = handle.list_transactions().await.map_err(|e| e.to_string())?;
    let mut csv = String::from(
        "id,ts,type,peer_id,provider,model,input_tokens,output_tokens,request_hash\n",
    );
    for row in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            row.id,
            row.ts,
            row.tx_type,
            row.peer_id.unwrap_or_default(),
            row.provider,
            row.model.unwrap_or_default(),
            row.input_tokens,
            row.output_tokens,
            row.request_hash
        ));
    }
    Ok(csv)
}

#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<bool, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    Ok(update.is_some())
}

#[tauri::command]
async fn set_sharing_override(state: State<'_, AppState>, sharing_override: String) -> Result<(), String> {
    let handle = get_p2p_handle(&state).await?;
    handle
        .set_sharing_override(sharing_override)
        .await
        .map_err(|e| e.to_string())
}

async fn get_p2p_handle(state: &State<'_, AppState>) -> Result<P2pHandle, String> {
    state
        .p2p_handle
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P subsystem still starting".to_string())
}

async fn start_proxy_inner(state: AppState) -> anyhow::Result<()> {
    let port = read_proxy_port(&state.db)?;
    let mut runtime = state.proxy_runtime.lock().await;

    if runtime.running {
        return Ok(());
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    runtime.running = true;
    runtime.port = port;
    runtime.shutdown_tx = Some(shutdown_tx);

    let app_state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = proxy::run_proxy_server(app_state.clone(), port, shutdown_rx).await {
            eprintln!("proxy stopped: {err}");
        }
        let mut runtime = app_state.proxy_runtime.lock().await;
        runtime.running = false;
        runtime.shutdown_tx = None;
    });

    Ok(())
}

fn read_proxy_port(db: &Db) -> anyhow::Result<u16> {
    let value = db
        .get_setting("proxy_port")?
        .unwrap_or_else(|| "47821".to_string());
    value
        .parse::<u16>()
        .with_context(|| format!("invalid proxy port in settings: {value}"))
}

fn app_db_path(handle: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = handle.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("tokenunion.db"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let db_path = app_db_path(app.handle())?;
            let db = Arc::new(Db::new(db_path)?);
            if db.get_setting("onboarding_completed")?.is_none() {
                db.set_setting("onboarding_completed", "0")?;
            }
            if db.get_setting("pref_auto_start")?.is_none() {
                db.set_setting("pref_auto_start", "0")?;
            }
            if db.get_setting("pref_notifications")?.is_none() {
                db.set_setting("pref_notifications", "1")?;
            }
            if db.get_setting("pref_appearance")?.is_none() {
                db.set_setting("pref_appearance", "dark")?;
            }
            let port = read_proxy_port(&db).unwrap_or(47821);

            let state = AppState {
                db: db.clone(),
                http: Client::new(),
                vault_password: Arc::new(RwLock::new(None)),
                proxy_runtime: Arc::new(Mutex::new(ProxyRuntime {
                    running: false,
                    port,
                    shutdown_tx: None,
                })),
                p2p_handle: Arc::new(RwLock::new(None)),
            };

            app.manage(state.clone());

            let open_i = MenuItemBuilder::new("Open Dashboard").id("open").build(app)?;
            let pause_i = MenuItemBuilder::new("Pause Sharing").id("pause").build(app)?;
            let leave_i = MenuItemBuilder::new("Leave Pool").id("leave").build(app)?;
            let quit_i = MenuItemBuilder::new("Quit").id("quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&open_i)
                .item(&pause_i)
                .item(&leave_i)
                .separator()
                .item(&quit_i)
                .build()?;

            let app_handle = app.handle().clone();
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .on_menu_event(move |tray: &AppHandle<Wry>, event: MenuEvent| match event.id().as_ref() {
                    "open" => {
                        let _ = app_handle.get_webview_window("main").map(|w| {
                            let _ = w.show();
                            let _ = w.set_focus();
                        });
                    }
                    "pause" => {
                        let h = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(state) = h.try_state::<AppState>() {
                                if let Some(p2p) = state.p2p_handle.read().await.clone() {
                                    let _ = p2p.set_sharing_override("paused".to_string()).await;
                                }
                            }
                        });
                    }
                    "leave" => {
                        let h = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(state) = h.try_state::<AppState>() {
                                if let Some(p2p) = state.p2p_handle.read().await.clone() {
                                    let _ = p2p.leave_circle().await;
                                }
                            }
                        });
                    }
                    "quit" => {
                        tray.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon<Wry>, event: TrayIconEvent| {
                    if let TrayIconEvent::Click { button, .. } = event {
                        if button == tauri::tray::MouseButton::Left {
                            if let Some(w) = tray.app_handle().get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            tauri::async_runtime::spawn(async move {
                let _ = start_proxy_inner(state.clone()).await;
                match p2p::bootstrap(state.db.clone(), state.http.clone(), state.vault_password.clone()).await {
                    Ok(handle) => {
                        *state.p2p_handle.write().await = Some(handle.clone());

                        // schedule ticker updates availability every minute
                        let schedule_handle = handle.clone();
                        tokio::spawn(async move {
                            loop {
                                let _ = schedule_handle.evaluate_schedule_tick().await;
                                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                            }
                        });
                    }
                    Err(err) => {
                        eprintln!("failed to start p2p runtime: {err}");
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            unlock_vault,
            set_provider_key,
            set_anthropic_key,
            create_anthropic_oauth_authorize_url,
            exchange_anthropic_oauth_code,
            list_oauth_sessions,
            delete_oauth_session,
            get_anthropic_oauth_config,
            set_anthropic_oauth_config,
            list_keys,
            delete_key,
            set_proxy_port,
            get_proxy_port,
            start_proxy,
            stop_proxy,
            get_dashboard_stats,
            get_local_node,
            update_profile,
            create_invite_url,
            join_invite,
            list_peers,
            list_token_messages,
            get_schedule_config,
            set_schedule_config,
            evaluate_schedule_tick,
            get_pool_status,
            get_ledger_transactions,
            get_fair_use_7d,
            get_audit_log,
            get_security_events,
            get_rate_limit_stats,
            set_peer_rate_limit,
            leave_circle,
            set_content_filter_patterns,
            get_relay_mode,
            set_relay_mode,
            get_onboarding_completed,
            complete_onboarding,
            get_app_preferences,
            set_app_preferences,
            export_ledger_csv,
            check_for_updates,
            set_sharing_override
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
