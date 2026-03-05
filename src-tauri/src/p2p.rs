use std::{collections::HashMap, io::Write, str::FromStr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use age::secrecy::{ExposeSecret, SecretString};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::StreamExt;
use libp2p::{
    dcutr,
    gossipsub,
    identity,
    mdns,
    multiaddr::Protocol,
    relay,
    request_response,
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, StreamProtocol, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, RwLock};
use url::Url;
use uuid::Uuid;

use crate::{
    db::{Db, FairUseRecord, LedgerGossipRecord, LocalIdentityRecord, PeerRecord, PoolStatusRecord, ScheduleConfig, TokenMessageRecord, TransactionInsert, TransactionRecord},
    tracker::usage_from_json_body,
    vault,
};

const TOKEN_PROTOCOL: &str = "/tokenunion/token/2";
const LEDGER_TOPIC: &str = "tokenunion/ledger/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalNodeDto {
    pub peer_id: String,
    pub display_name: String,
    pub timezone: String,
    pub availability_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitePayload {
    pub peer_id: String,
    pub display_name: String,
    pub timezone: String,
    pub multiaddr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantDecision {
    pub peer_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRelayRequest {
    pub request_hash: String,
    pub provider: String,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRelayResponse {
    pub request_hash: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body_b64: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub request_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenWireMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub request_hash: Option<String>,
    pub requester_id: String,
    pub amount: Option<i64>,
    pub granted: Option<bool>,
    pub reason: Option<String>,
    pub availability_state: Option<String>,
    pub daily_limit_tokens: Option<i64>,
    pub daily_used_tokens: Option<i64>,
    pub proxy_request: Option<ProxyRelayRequest>,
    pub proxy_response: Option<ProxyRelayResponse>,
    pub request_nonce: Option<String>,
    pub request_ts: Option<i64>,
    pub signer_public_key_b64: Option<String>,
    pub signature_b64: Option<String>,
    pub circle_key_recipient: Option<String>,
    pub circle_key_box: Option<String>,
    pub ledger_entries: Option<Vec<LedgerGossipRecord>>,
}

impl TokenWireMessage {
    fn with_amount(mut self, amount: Option<i64>) -> Self {
        self.amount = amount;
        self
    }
    fn with_granted(mut self, granted: bool) -> Self {
        self.granted = Some(granted);
        self
    }
    fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.to_string());
        self
    }
    fn with_reason_opt(mut self, reason: Option<String>) -> Self {
        self.reason = reason;
        self
    }
    fn with_proxy_response(mut self, response: Option<ProxyRelayResponse>) -> Self {
        self.proxy_response = response;
        self
    }
    fn with_circle_key_box(mut self, boxed: Option<String>) -> Self {
        self.circle_key_box = boxed;
        self
    }
    fn with_ledger_entries(mut self, entries: Option<Vec<LedgerGossipRecord>>) -> Self {
        self.ledger_entries = entries;
        self
    }
}

#[derive(Debug)]
enum P2pCommand {
    JoinInvite(String),
    BroadcastAvailability,
    RequestGrant {
        request_hash: String,
        amount: i64,
        responder: oneshot::Sender<Option<GrantDecision>>,
    },
    GrantTimeout { request_hash: String },
    ProxyCall {
        peer_id: String,
        payload: ProxyRelayRequest,
        responder: oneshot::Sender<Result<ProxyRelayResponse, String>>,
    },
    PublishLedgerTx {
        tx: TransactionInsert,
    },
    SetProfile { display_name: String, timezone: String },
    LeaveCircle,
}

#[derive(Clone)]
pub struct P2pHandle {
    local: Arc<RwLock<LocalNodeDto>>,
    listen_addr: Arc<RwLock<Option<Multiaddr>>>,
    relay_addr: Arc<RwLock<Option<String>>>,
    db: Arc<Db>,
    tx: mpsc::Sender<P2pCommand>,
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "TokenBehaviourEvent")]
struct TokenBehaviour {
    request_response: request_response::json::Behaviour<TokenWireMessage, TokenWireMessage>,
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
}

#[derive(Debug)]
enum TokenBehaviourEvent {
    RequestResponse(request_response::Event<TokenWireMessage, TokenWireMessage>),
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    Relay(relay::client::Event),
    Dcutr(dcutr::Event),
}

impl From<request_response::Event<TokenWireMessage, TokenWireMessage>> for TokenBehaviourEvent {
    fn from(value: request_response::Event<TokenWireMessage, TokenWireMessage>) -> Self {
        Self::RequestResponse(value)
    }
}

impl From<mdns::Event> for TokenBehaviourEvent {
    fn from(value: mdns::Event) -> Self {
        Self::Mdns(value)
    }
}

impl From<gossipsub::Event> for TokenBehaviourEvent {
    fn from(value: gossipsub::Event) -> Self {
        Self::Gossipsub(value)
    }
}

impl From<relay::client::Event> for TokenBehaviourEvent {
    fn from(value: relay::client::Event) -> Self {
        Self::Relay(value)
    }
}

impl From<dcutr::Event> for TokenBehaviourEvent {
    fn from(value: dcutr::Event) -> Self {
        Self::Dcutr(value)
    }
}

pub async fn bootstrap(
    db: Arc<Db>,
    http: reqwest::Client,
    vault_password: Arc<RwLock<Option<String>>>,
) -> Result<P2pHandle> {
    let identity = ensure_identity(&db)?;
    let keypair = decode_identity_keypair(&identity.encrypted_private_key)?;
    let peer_id = PeerId::from(keypair.public());

    let relay_addr = db.get_setting("relay_multiaddr")?;

    let local = Arc::new(RwLock::new(LocalNodeDto {
        peer_id: peer_id.to_string(),
        display_name: identity.display_name,
        timezone: identity.timezone,
        availability_state: db
            .get_local_availability_state()
            .unwrap_or_else(|_| "available".to_string()),
    }));

    let listen_addr = Arc::new(RwLock::new(None));
    let relay_shared = Arc::new(RwLock::new(relay_addr.clone()));

    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(run_swarm(
        db.clone(),
        keypair,
        local.clone(),
        listen_addr.clone(),
        relay_shared.clone(),
        http,
        vault_password,
        tx.clone(),
        rx,
    ));

    Ok(P2pHandle {
        local,
        listen_addr,
        relay_addr: relay_shared,
        db,
        tx,
    })
}

impl P2pHandle {
    pub async fn local_node(&self) -> Result<LocalNodeDto> {
        Ok(self.local.read().await.clone())
    }

    pub async fn evaluate_schedule_tick(&self) -> Result<String> {
        let state = self.db.evaluate_schedule_availability()?;
        {
            let mut local = self.local.write().await;
            local.availability_state = state.clone();
        }
        self.tx
            .send(P2pCommand::BroadcastAvailability)
            .await
            .context("p2p service is not running")?;
        Ok(state)
    }

    pub async fn set_schedule_config(
        &self,
        timezone: String,
        weekly_active_bitmap: String,
        sharing_override: String,
    ) -> Result<()> {
        self.db
            .set_schedule_config(&timezone, &weekly_active_bitmap, &sharing_override)?;
        self.evaluate_schedule_tick().await?;
        Ok(())
    }

    pub async fn set_sharing_override(&self, sharing_override: String) -> Result<()> {
        let cfg = self.db.get_schedule_config()?;
        self.db
            .set_schedule_config(&cfg.timezone, &cfg.weekly_active_bitmap, &sharing_override)?;
        self.evaluate_schedule_tick().await?;
        Ok(())
    }

    pub fn get_schedule_config(&self) -> Result<ScheduleConfig> {
        Ok(self.db.get_schedule_config()?)
    }

    pub async fn set_profile(&self, display_name: String, timezone: String) -> Result<()> {
        self.db.update_local_profile(&display_name, &timezone)?;
        {
            let mut local = self.local.write().await;
            local.display_name = display_name.clone();
            local.timezone = timezone.clone();
        }
        self.tx
            .send(P2pCommand::SetProfile {
                display_name,
                timezone,
            })
            .await
            .context("p2p service is not running")?;
        Ok(())
    }

    pub async fn create_invite_url(&self) -> Result<String> {
        let local = self.local.read().await.clone();
        let listen_addr = self
            .listen_addr
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("No active listen address yet"))?;

        let final_addr = if let Some(relay) = self.relay_addr.read().await.clone() {
            let relay_base: Multiaddr = relay.parse().context("invalid relay address in settings")?;
            relay_base
                .with(Protocol::P2pCircuit)
                .with(Protocol::P2p(
                    local
                        .peer_id
                        .parse::<PeerId>()
                        .context("invalid local peer id")?,
                ))
                .to_string()
        } else {
            listen_addr.to_string()
        };

        let payload = InvitePayload {
            peer_id: local.peer_id,
            display_name: local.display_name,
            timezone: local.timezone,
            multiaddr: final_addr,
        };

        let encoded = urlencoding::encode(&serde_json::to_string(&payload)?).to_string();
        Ok(format!("tokenunion://invite?data={encoded}"))
    }

    pub async fn join_invite(&self, invite: String) -> Result<()> {
        self.tx
            .send(P2pCommand::JoinInvite(invite))
            .await
            .context("p2p service is not running")?;
        Ok(())
    }

    pub async fn request_peer_grant(&self, request_hash: String, amount: i64) -> Result<Option<GrantDecision>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(P2pCommand::RequestGrant {
                request_hash,
                amount,
                responder: tx,
            })
            .await
            .context("p2p service is not running")?;
        Ok(rx.await.unwrap_or(None))
    }

    pub async fn proxy_via_peer(
        &self,
        peer_id: String,
        payload: ProxyRelayRequest,
    ) -> Result<ProxyRelayResponse> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(P2pCommand::ProxyCall {
                peer_id,
                payload,
                responder: tx,
            })
            .await
            .context("p2p service is not running")?;

        rx.await
            .map_err(|_| anyhow!("peer proxy response channel closed"))?
            .map_err(|e| anyhow!(e))
    }

    pub async fn publish_transaction(&self, tx: TransactionInsert) -> Result<()> {
        self.tx
            .send(P2pCommand::PublishLedgerTx { tx })
            .await
            .context("p2p service is not running")?;
        Ok(())
    }

    pub async fn broadcast_availability(&self) -> Result<()> {
        self.tx
            .send(P2pCommand::BroadcastAvailability)
            .await
            .context("p2p service is not running")?;
        Ok(())
    }

    pub async fn list_peers(&self) -> Result<Vec<PeerRecord>> {
        Ok(self.db.list_peers()?)
    }

    pub fn set_peer_rate_limit(&self, peer_id: String, max_per_min: i64) -> Result<()> {
        self.db.set_peer_rate_limit(&peer_id, max_per_min)?;
        Ok(())
    }

    pub fn list_rate_limit_stats(&self) -> Result<Vec<crate::db::RateLimitStat>> {
        Ok(self.db.list_rate_limit_stats()?)
    }

    pub async fn list_messages(&self) -> Result<Vec<TokenMessageRecord>> {
        Ok(self.db.list_recent_peer_messages(50)?)
    }

    pub async fn list_transactions(&self) -> Result<Vec<TransactionRecord>> {
        Ok(self.db.list_transactions(100)?)
    }

    pub async fn fair_use_7d(&self) -> Result<Vec<FairUseRecord>> {
        Ok(self.db.fair_use_7d()?)
    }

    pub async fn pool_status(&self) -> Result<Vec<PoolStatusRecord>> {
        let local = self.local.read().await.clone();
        Ok(self
            .db
            .pool_status(&local.peer_id, &local.display_name, &local.timezone)?)
    }

    pub async fn leave_circle(&self) -> Result<()> {
        self.tx
            .send(P2pCommand::LeaveCircle)
            .await
            .context("p2p service is not running")?;
        self.db.clear_all_peers()?;
        let current = self.local.read().await.clone();
        let new_keypair = identity::Keypair::generate_ed25519();
        let new_peer_id = PeerId::from(new_keypair.public()).to_string();
        let encoded = STANDARD.encode(new_keypair.to_protobuf_encoding()?);
        let encrypted = encrypt_identity_blob(&encoded)?;
        self.db.set_local_identity(
            &new_peer_id,
            &encrypted,
            &current.display_name,
            &current.timezone,
        )?;
        {
            let mut local = self.local.write().await;
            local.peer_id = new_peer_id;
        }
        self.db
            .insert_security_event("revocation", "leave_circle invoked; new identity generated (restart app to bind new swarm identity)")?;
        Ok(())
    }

    pub fn list_audit_log(&self) -> Result<Vec<crate::db::AuditLogRecord>> {
        Ok(self.db.list_audit_log(200)?)
    }

    pub fn list_security_events(&self) -> Result<Vec<crate::db::SecurityEventRecord>> {
        Ok(self.db.list_security_events(100)?)
    }
}

fn ensure_identity(db: &Db) -> Result<LocalIdentityRecord> {
    if let Some(existing) = db.get_local_identity()? {
        return Ok(existing);
    }

    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public()).to_string();
    let encoded = STANDARD.encode(keypair.to_protobuf_encoding()?);
    let encrypted = encrypt_identity_blob(&encoded)?;

    let timezone = chrono::Local::now().offset().to_string();
    db.set_local_identity(&peer_id, &encrypted, "Anonymous", &timezone)?;

    db.get_local_identity()?
        .ok_or_else(|| anyhow!("failed to persist local identity"))
}

fn decode_identity_keypair(encrypted_private_key: &str) -> Result<identity::Keypair> {
    let encoded = decrypt_identity_blob(encrypted_private_key)?;
    let bytes = STANDARD.decode(encoded)?;
    let keypair = identity::Keypair::from_protobuf_encoding(&bytes)?;
    Ok(keypair)
}

fn identity_password() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    format!("tokenunion-local-identity::{user}::{}", std::env::consts::OS)
}

fn identity_device_salt() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string());
    format!("identity::{host}")
}

fn encrypt_identity_blob(raw: &str) -> Result<String> {
    vault::encrypt_api_key(raw, &identity_password(), &identity_device_salt())
}

fn decrypt_identity_blob(ciphertext: &str) -> Result<String> {
    vault::decrypt_api_key(ciphertext, &identity_password(), &identity_device_salt())
}

async fn run_swarm(
    db: Arc<Db>,
    keypair: identity::Keypair,
    local: Arc<RwLock<LocalNodeDto>>,
    listen_addr_shared: Arc<RwLock<Option<Multiaddr>>>,
    relay_addr: Arc<RwLock<Option<String>>>,
    http: reqwest::Client,
    vault_password: Arc<RwLock<Option<String>>>,
    command_tx: mpsc::Sender<P2pCommand>,
    mut cmd_rx: mpsc::Receiver<P2pCommand>,
) {
    let local_peer_id = PeerId::from(keypair.public());
    let signer_key = keypair.clone();

    let mut swarm = match build_swarm(keypair).await {
        Ok(s) => s,
        Err(err) => {
            eprintln!("failed to bootstrap p2p swarm: {err}");
            return;
        }
    };

    if let Err(err) = swarm.listen_on(
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("valid listen multiaddr"),
    ) {
        eprintln!("failed to listen for p2p: {err}");
    }

    let relay_mode = db
        .get_setting("relay_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| "self_hosted".to_string());
    if relay_mode != "off" {
        if let Ok(Some(relay_multiaddr)) = db.get_setting("relay_multiaddr") {
            *relay_addr.write().await = Some(relay_multiaddr.clone());
            if let Ok(addr) = relay_multiaddr.parse::<Multiaddr>() {
                let _ = swarm.dial(addr);
            }
        }
    }

    let mut grant_waiters: HashMap<String, oneshot::Sender<Option<GrantDecision>>> = HashMap::new();
    let mut grant_outbound: HashMap<request_response::OutboundRequestId, (String, String)> = HashMap::new();
    let mut proxy_outbound: HashMap<request_response::OutboundRequestId, oneshot::Sender<Result<ProxyRelayResponse, String>>> = HashMap::new();
    let mut circle_key_outbound: HashMap<request_response::OutboundRequestId, String> = HashMap::new();
    let mut relay_auto_disabled = false;

    loop {
        tokio::select! {
            maybe_cmd = cmd_rx.recv() => {
                if let Some(cmd) = maybe_cmd {
                    if let Err(err) = handle_command(
                        &db,
                        &local,
                        local_peer_id,
                        &mut swarm,
                        cmd,
                        &command_tx,
                        &signer_key,
                        &mut grant_waiters,
                        &mut grant_outbound,
                        &mut proxy_outbound,
                        &mut circle_key_outbound,
                    ).await {
                        eprintln!("p2p command failed: {err}");
                    }
                } else {
                    break;
                }
            }
            event = swarm.select_next_some() => {
                if let Err(err) = handle_swarm_event(
                    &db,
                    &local,
                    local_peer_id,
                    &mut swarm,
                    event,
                    &mut grant_waiters,
                    &mut grant_outbound,
                    &mut proxy_outbound,
                    &mut circle_key_outbound,
                    &http,
                    &vault_password,
                    &listen_addr_shared,
                    &signer_key,
                    &mut relay_auto_disabled,
                ).await {
                    eprintln!("p2p event error: {err}");
                }
            }
        }
    }
}

async fn build_swarm(keypair: identity::Keypair) -> Result<libp2p::Swarm<TokenBehaviour>> {
    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default().nodelay(true),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_relay_client(libp2p::noise::Config::new, libp2p::yamux::Config::default)?
        .with_behaviour(|key, relay_behaviour| {
            let protocols = [(
                StreamProtocol::new(TOKEN_PROTOCOL),
                request_response::ProtocolSupport::Full,
            )];

            let request_response = request_response::json::Behaviour::<
                TokenWireMessage,
                TokenWireMessage,
            >::new(protocols, request_response::Config::default());

            let gossip_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(5))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .build()
                .context("gossipsub config")?;
            let mut gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossip_config,
            )
            .map_err(|e| anyhow!("gossipsub init: {e}"))?;
            gossipsub
                .subscribe(&gossipsub::IdentTopic::new(LEDGER_TOPIC))
                .context("subscribe ledger topic")?;

            Ok(TokenBehaviour {
                request_response,
                gossipsub,
                mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?,
                relay: relay_behaviour,
                dcutr: dcutr::Behaviour::new(key.public().to_peer_id()),
            })
        })?
        .build();

    Ok(swarm)
}

async fn handle_command(
    db: &Arc<Db>,
    local: &Arc<RwLock<LocalNodeDto>>,
    local_peer_id: PeerId,
    swarm: &mut libp2p::Swarm<TokenBehaviour>,
    cmd: P2pCommand,
    command_tx: &mpsc::Sender<P2pCommand>,
    signer_key: &identity::Keypair,
    grant_waiters: &mut HashMap<String, oneshot::Sender<Option<GrantDecision>>>,
    grant_outbound: &mut HashMap<request_response::OutboundRequestId, (String, String)>,
    proxy_outbound: &mut HashMap<request_response::OutboundRequestId, oneshot::Sender<Result<ProxyRelayResponse, String>>>,
    circle_key_outbound: &mut HashMap<request_response::OutboundRequestId, String>,
) -> Result<()> {
    match cmd {
        P2pCommand::JoinInvite(invite) => {
            let payload = parse_invite(&invite)?;
            let peer_id: PeerId = payload.peer_id.parse().context("invalid peer id in invite")?;
            let addr: Multiaddr = payload.multiaddr.parse().context("invalid multiaddr in invite")?;

            db.upsert_peer(
                &payload.peer_id,
                &payload.display_name,
                &payload.timezone,
                Some(&payload.multiaddr),
                false,
            )?;

            swarm.behaviour_mut().request_response.add_address(&peer_id, addr.clone());
            swarm.dial(addr)?;

            let join_identity = age::x25519::Identity::generate();
            let recipient = join_identity.to_public().to_string();
            let key_req = TokenWireMessage {
                msg_type: "circle_key_request".to_string(),
                request_hash: None,
                requester_id: local_peer_id.to_string(),
                amount: None,
                granted: None,
                reason: None,
                availability_state: None,
                daily_limit_tokens: None,
                daily_used_tokens: None,
                proxy_request: None,
                proxy_response: None,
                request_nonce: None,
                request_ts: None,
                signer_public_key_b64: None,
                signature_b64: None,
                circle_key_recipient: Some(recipient),
                circle_key_box: None,
                ledger_entries: None,
            };
            let key_req_id = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, key_req);
            circle_key_outbound.insert(
                key_req_id,
                join_identity.to_string().expose_secret().to_string(),
            );

            let me = local.read().await.clone();
            let announce = TokenWireMessage {
                msg_type: "availability_update".to_string(),
                request_hash: None,
                requester_id: me.peer_id,
                amount: None,
                granted: None,
                reason: None,
                availability_state: Some(me.availability_state),
                daily_limit_tokens: Some(db.local_limit_tokens().unwrap_or(100000)),
                daily_used_tokens: Some(db.local_used_tokens().unwrap_or(0)),
                proxy_request: None,
                proxy_response: None,
                request_nonce: None,
                request_ts: None,
                signer_public_key_b64: None,
                signature_b64: None,
                circle_key_recipient: None,
                circle_key_box: None,
                ledger_entries: None,
            };
            swarm.behaviour_mut().request_response.send_request(&peer_id, announce);
        }
        P2pCommand::BroadcastAvailability => {
            let me = local.read().await.clone();
            let message = TokenWireMessage {
                msg_type: "availability_update".to_string(),
                request_hash: None,
                requester_id: me.peer_id,
                amount: None,
                granted: None,
                reason: None,
                availability_state: Some(me.availability_state),
                daily_limit_tokens: Some(db.local_limit_tokens().unwrap_or(100000)),
                daily_used_tokens: Some(db.local_used_tokens().unwrap_or(0)),
                proxy_request: None,
                proxy_response: None,
                request_nonce: None,
                request_ts: None,
                signer_public_key_b64: None,
                signature_b64: None,
                circle_key_recipient: None,
                circle_key_box: None,
                ledger_entries: None,
            };
            for peer in db.list_online_peers()? {
                if let Ok(peer_id) = peer.peer_id.parse::<PeerId>() {
                    swarm.behaviour_mut().request_response.send_request(&peer_id, message.clone());
                }
            }
        }
        P2pCommand::RequestGrant {
            request_hash,
            amount,
            responder,
        } => {
            if db.get_circle_key_plain()?.is_none() {
                let _ = responder.send(None);
                return Ok(());
            }
            grant_waiters.insert(request_hash.clone(), responder);
            let nonce = Uuid::new_v4().to_string();
            let ts = chrono::Utc::now().timestamp();
            let payload = signing_payload(&local_peer_id.to_string(), &request_hash, &nonce, ts);
            let signature = signer_key.sign(payload.as_bytes()).context("failed to sign token_request")?;
            let public_key = signer_key.public().encode_protobuf();
            for peer in db.list_online_peers()? {
                if let Ok(peer_id) = peer.peer_id.parse::<PeerId>() {
                    let msg = TokenWireMessage {
                        msg_type: "token_request".to_string(),
                        request_hash: Some(request_hash.clone()),
                        requester_id: local_peer_id.to_string(),
                        amount: Some(amount),
                        granted: None,
                        reason: None,
                        availability_state: None,
                        daily_limit_tokens: None,
                        daily_used_tokens: None,
                        proxy_request: None,
                        proxy_response: None,
                        request_nonce: Some(nonce.clone()),
                        request_ts: Some(ts),
                        signer_public_key_b64: Some(STANDARD.encode(public_key.clone())),
                        signature_b64: Some(STANDARD.encode(signature.clone())),
                        circle_key_recipient: None,
                        circle_key_box: None,
                        ledger_entries: None,
                    };
                    let out_id = swarm.behaviour_mut().request_response.send_request(&peer_id, msg);
                    grant_outbound.insert(out_id, (request_hash.clone(), peer.peer_id.clone()));
                }
            }

            let timeout_hash = request_hash.clone();
            let tx = command_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = tx.send(P2pCommand::GrantTimeout { request_hash: timeout_hash }).await;
            });
        }
        P2pCommand::GrantTimeout { request_hash } => {
            if let Some(waiter) = grant_waiters.remove(&request_hash) {
                let _ = waiter.send(None);
            }
            grant_outbound.retain(|_, (hash, _)| hash != &request_hash);
        }
        P2pCommand::ProxyCall {
            peer_id,
            payload,
            responder,
        } => {
            if db.get_circle_key_plain()?.is_none() {
                let _ = responder.send(Err("circle key missing".to_string()));
                return Ok(());
            }
            let peer: PeerId = peer_id.parse().context("invalid peer id")?;
            let msg = TokenWireMessage {
                msg_type: "proxy_call".to_string(),
                request_hash: Some(payload.request_hash.clone()),
                requester_id: local_peer_id.to_string(),
                amount: None,
                granted: None,
                reason: None,
                availability_state: None,
                daily_limit_tokens: None,
                daily_used_tokens: None,
                proxy_request: Some(payload),
                proxy_response: None,
                request_nonce: None,
                request_ts: None,
                signer_public_key_b64: None,
                signature_b64: None,
                circle_key_recipient: None,
                circle_key_box: None,
                ledger_entries: None,
            };
            let out_id = swarm.behaviour_mut().request_response.send_request(&peer, msg);
            proxy_outbound.insert(out_id, responder);
        }
        P2pCommand::PublishLedgerTx { tx } => {
            let envelope = build_ledger_envelope(db, signer_key, local_peer_id, &tx)?;
            let topic = gossipsub::IdentTopic::new(LEDGER_TOPIC);
            swarm
                .behaviour_mut()
                .gossipsub
                .publish(topic, serde_json::to_vec(&envelope)?)
                .context("gossip publish failed")?;
            let _ = db.upsert_ledger_gossip(&envelope)?;
        }
        P2pCommand::SetProfile { display_name, timezone } => {
            let _ = (display_name, timezone);
        }
        P2pCommand::LeaveCircle => {
            let _ = db.rotate_circle_key();
            for peer in db.list_online_peers()? {
                if let Ok(peer_id) = peer.peer_id.parse::<PeerId>() {
                    let msg = TokenWireMessage {
                        msg_type: "leave_circle".to_string(),
                        request_hash: None,
                        requester_id: local_peer_id.to_string(),
                        amount: None,
                        granted: None,
                        reason: Some("peer_revoked".to_string()),
                        availability_state: None,
                        daily_limit_tokens: None,
                        daily_used_tokens: None,
                        proxy_request: None,
                        proxy_response: None,
                        request_nonce: None,
                        request_ts: None,
                        signer_public_key_b64: None,
                        signature_b64: None,
                        circle_key_recipient: None,
                        circle_key_box: None,
                        ledger_entries: None,
                    };
                    swarm.behaviour_mut().request_response.send_request(&peer_id, msg);
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_swarm_event(
    db: &Arc<Db>,
    local: &Arc<RwLock<LocalNodeDto>>,
    _local_peer_id: PeerId,
    swarm: &mut libp2p::Swarm<TokenBehaviour>,
    event: SwarmEvent<TokenBehaviourEvent>,
    grant_waiters: &mut HashMap<String, oneshot::Sender<Option<GrantDecision>>>,
    grant_outbound: &mut HashMap<request_response::OutboundRequestId, (String, String)>,
    proxy_outbound: &mut HashMap<request_response::OutboundRequestId, oneshot::Sender<Result<ProxyRelayResponse, String>>>,
    circle_key_outbound: &mut HashMap<request_response::OutboundRequestId, String>,
    http: &reqwest::Client,
    vault_password: &Arc<RwLock<Option<String>>>,
    listen_addr_shared: &Arc<RwLock<Option<Multiaddr>>>,
    signer_key: &identity::Keypair,
    relay_auto_disabled: &mut bool,
) -> Result<()> {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            *listen_addr_shared.write().await = Some(address.clone());
            println!("p2p listening on {address}");
        }
        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
            let peer = peer_id.to_string();
            if let Some(existing) = db.list_peers()?.into_iter().find(|p| p.peer_id == peer) {
                db.upsert_peer(
                    &existing.peer_id,
                    &existing.display_name,
                    &existing.timezone,
                    existing.multiaddr.as_deref(),
                    true,
                )?;
            } else {
                db.upsert_peer(&peer, "Unknown", "UTC", None, true)?;
            }
            if !*relay_auto_disabled && !endpoint_is_relay(&endpoint) {
                *relay_auto_disabled = true;
                let _ = db.set_setting("relay_mode", "off");
                let _ = db.insert_security_event("relay_auto_disabled", "direct path established; relay circuit disabled");
            }
            let cursor = db.max_lamport().unwrap_or(0);
            let sync_req = TokenWireMessage {
                msg_type: "ledger_sync_request".to_string(),
                request_hash: Some(cursor.to_string()),
                requester_id: local.read().await.peer_id.clone(),
                amount: None,
                granted: None,
                reason: None,
                availability_state: None,
                daily_limit_tokens: None,
                daily_used_tokens: None,
                proxy_request: None,
                proxy_response: None,
                request_nonce: None,
                request_ts: None,
                signer_public_key_b64: None,
                signature_b64: None,
                circle_key_recipient: None,
                circle_key_box: None,
                ledger_entries: None,
            };
            swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, sync_req);
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            db.set_peer_status(&peer_id.to_string(), false)?;
        }
        SwarmEvent::Behaviour(TokenBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, addr) in list {
                swarm.behaviour_mut().request_response.add_address(&peer_id, addr.clone());
                let _ = swarm.dial(addr.clone());
                db.upsert_peer(&peer_id.to_string(), "LAN Peer", "UTC", Some(&addr.to_string()), true)?;
            }
        }
        SwarmEvent::Behaviour(TokenBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            for (peer_id, _) in list {
                db.set_peer_status(&peer_id.to_string(), false)?;
            }
        }
        SwarmEvent::Behaviour(TokenBehaviourEvent::RequestResponse(ev)) => {
            match ev {
                request_response::Event::Message { peer, message, .. } => {
                    match message {
                        request_response::Message::Request { request, channel, .. } => {
                            let request_clone = request.clone();
                            let response = handle_incoming_request(db, local, http, vault_password, &peer, request).await?;
                            if request_clone.msg_type == "proxy_call" {
                                if let Some(proxy_req) = request_clone.proxy_request {
                                    if let Some(proxy_res) = response.proxy_response.as_ref() {
                                        let tx = TransactionInsert {
                                            tx_type: "lent".to_string(),
                                            peer_id: Some(request_clone.requester_id.clone()),
                                            provider: proxy_req.provider,
                                            model: proxy_res.model.clone(),
                                            input_tokens: proxy_res.input_tokens,
                                            output_tokens: proxy_res.output_tokens,
                                            request_hash: proxy_req.request_hash,
                                        };
                                        let local_pid = local.read().await.peer_id.clone();
                                        if let Ok(local_peer_id) = local_pid.parse::<PeerId>() {
                                            if let Ok(envelope) = build_ledger_envelope(db, signer_key, local_peer_id, &tx) {
                                            let _ = db.upsert_ledger_gossip(&envelope);
                                            let _ = swarm
                                                .behaviour_mut()
                                                .gossipsub
                                                .publish(gossipsub::IdentTopic::new(LEDGER_TOPIC), serde_json::to_vec(&envelope).unwrap_or_default());
                                            }
                                        }
                                    }
                                }
                            }
                            swarm
                                .behaviour_mut()
                                .request_response
                                .send_response(channel, response)
                                .map_err(|e| anyhow!("send_response failed: {e:?}"))?;
                        }
                        request_response::Message::Response { request_id, response } => {
                            if let Some(join_secret) = circle_key_outbound.remove(&request_id) {
                                if response.msg_type == "circle_key_share" {
                                    if let Some(cipher_b64) = response.circle_key_box.as_deref() {
                                        if let Ok(circle_key) = decrypt_circle_key_box(cipher_b64, &join_secret) {
                                            let _ = db.set_circle_key_plain(&circle_key, 1);
                                            let _ = db.insert_security_event("circle_key_received", &format!("received from {}", peer));
                                        }
                                    }
                                }
                            }
                            if let Some((req_hash, peer_id)) = grant_outbound.remove(&request_id) {
                                if response.msg_type == "token_grant" && response.granted == Some(true) {
                                    if let Some(waiter) = grant_waiters.remove(&req_hash) {
                                        let _ = waiter.send(Some(GrantDecision {
                                            peer_id: peer_id.clone(),
                                            reason: response.reason.clone(),
                                        }));
                                    }

                                    for (out_id, (other_hash, other_peer)) in grant_outbound.clone().iter() {
                                        if other_hash == &req_hash && other_peer != &peer_id {
                                            if let Ok(other_pid) = other_peer.parse::<PeerId>() {
                                                let cancel = TokenWireMessage {
                                                    msg_type: "cancel_request".to_string(),
                                                    request_hash: Some(req_hash.clone()),
                                                    requester_id: local.read().await.peer_id.clone(),
                                                    amount: None,
                                                    granted: None,
                                                    reason: Some("winner_selected".to_string()),
                                                    availability_state: None,
                                                    daily_limit_tokens: None,
                                                    daily_used_tokens: None,
                                                    proxy_request: None,
                                                    proxy_response: None,
                                                    request_nonce: None,
                                                    request_ts: None,
                                                    signer_public_key_b64: None,
                                                    signature_b64: None,
                circle_key_recipient: None,
                circle_key_box: None,
                ledger_entries: None,
                                                };
                                                swarm.behaviour_mut().request_response.send_request(&other_pid, cancel);
                                            }
                                            let _ = grant_outbound.remove(out_id);
                                        }
                                    }
                                }
                            }

                            if let Some(waiter) = proxy_outbound.remove(&request_id) {
                                if let Some(proxy_response) = response.proxy_response {
                                    let _ = waiter.send(Ok(proxy_response));
                                } else {
                                    let _ = waiter.send(Err(
                                        response
                                            .reason
                                            .clone()
                                            .unwrap_or_else(|| "invalid proxy response".to_string()),
                                    ));
                                }
                            }

                            if response.msg_type == "ledger_sync_response" {
                                if let Some(entries) = response.ledger_entries.as_ref() {
                                    for entry in entries {
                                        let _ = apply_ledger_gossip(db, entry.clone());
                                    }
                                }
                            }

                            db.insert_peer_message(
                                "inbound",
                                &peer.to_string(),
                                &response.msg_type,
                                response.amount.unwrap_or_default(),
                                response.granted,
                                response.reason.as_deref(),
                            )?;
                        }
                    }
                }
                request_response::Event::OutboundFailure { request_id, peer, error } => {
                    eprintln!("request_response outbound failure to {peer}: {error}");
                    let _ = db.set_peer_status(&peer.to_string(), false);
                    if let Some(waiter) = proxy_outbound.remove(&request_id) {
                        let _ = waiter.send(Err(format!("proxy call failed: {error}")));
                    }
                    if let Some((req_hash, _)) = grant_outbound.remove(&request_id) {
                        if !grant_outbound.values().any(|(hash, _)| hash == &req_hash) {
                            if let Some(waiter) = grant_waiters.remove(&req_hash) {
                                let _ = waiter.send(None);
                            }
                        }
                    }
                }
                request_response::Event::InboundFailure { peer, error, .. } => {
                    eprintln!("request_response inbound failure from {peer}: {error}");
                }
                request_response::Event::ResponseSent { .. } => {}
            }
        }
        SwarmEvent::Behaviour(TokenBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
            if let Ok(envelope) = serde_json::from_slice::<LedgerGossipRecord>(&message.data) {
                let _ = apply_ledger_gossip(db, envelope);
            }
        }
        SwarmEvent::Behaviour(TokenBehaviourEvent::Relay(_)) => {}
        SwarmEvent::Behaviour(TokenBehaviourEvent::Dcutr(_)) => {}
        _ => {}
    }

    Ok(())
}

async fn handle_incoming_request(
    db: &Arc<Db>,
    local: &Arc<RwLock<LocalNodeDto>>,
    http: &reqwest::Client,
    vault_password: &Arc<RwLock<Option<String>>>,
    peer: &PeerId,
    request: TokenWireMessage,
) -> Result<TokenWireMessage> {
    let local_peer_id = local.read().await.peer_id.clone();
    db.insert_peer_message(
        "inbound",
        &peer.to_string(),
        &request.msg_type,
        request.amount.unwrap_or_default(),
        request.granted,
        request.reason.as_deref(),
    )?;

    match request.msg_type.as_str() {
        "circle_key_request" => {
            let recipient = request
                .circle_key_recipient
                .as_deref()
                .ok_or_else(|| anyhow!("missing circle key recipient"))?;
            let circle_key = db.ensure_circle_key()?;
            let encrypted = encrypt_circle_key_for_recipient(&circle_key, recipient)?;
            Ok(base_wire(local_peer_id.clone(), request.request_hash, "circle_key_share")
                .with_granted(true)
                .with_reason("circle key shared")
                .with_circle_key_box(Some(encrypted)))
        }
        "availability_update" => {
            db.update_peer_availability(
                &peer.to_string(),
                request.availability_state.as_deref().unwrap_or("unknown"),
                request.daily_limit_tokens,
                request.daily_used_tokens,
            )?;
            Ok(base_wire(local_peer_id.clone(), request.request_hash, "ack").with_granted(true))
        }
        "ledger_sync_request" => {
            let cursor = request
                .request_hash
                .as_deref()
                .unwrap_or("0")
                .parse::<i64>()
                .unwrap_or(0);
            let entries = db.list_ledger_gossip_since(cursor, 200)?;
            Ok(base_wire(local_peer_id.clone(), request.request_hash, "ledger_sync_response")
                .with_granted(true)
                .with_ledger_entries(Some(entries)))
        }
        "token_request" => {
            if let Err(err) = validate_signed_token_request(&request, peer) {
                db.insert_security_event("signature_rejected", &format!("{}: {}", peer, err))?;
                return Ok(base_wire(local_peer_id.clone(), request.request_hash, "token_grant")
                    .with_amount(request.amount)
                    .with_granted(false)
                    .with_reason("invalid signature"));
            }
            let nonce = request.request_nonce.clone().unwrap_or_default();
            let ts_epoch = request.request_ts.unwrap_or_default();
            if !db.remember_nonce_if_fresh(&nonce, ts_epoch)? {
                db.insert_security_event("replay_rejected", &format!("{} nonce={}", peer, nonce))?;
                return Ok(base_wire(local_peer_id.clone(), request.request_hash, "token_grant")
                    .with_amount(request.amount)
                    .with_granted(false)
                    .with_reason("replay/timestamp rejected"));
            }
            let (allowed, count, limit) = db.check_and_increment_peer_rate_limit(&peer.to_string())?;
            if !allowed {
                db.insert_security_event(
                    "rate_limited",
                    &format!("peer {} exceeded limit {}/min with {}", peer, limit, count),
                )?;
                return Ok(base_wire(local_peer_id.clone(), request.request_hash, "token_grant")
                    .with_amount(request.amount)
                    .with_granted(false)
                    .with_reason("rate limit exceeded"));
            }
            let amount = request.amount.unwrap_or(1000);
            let (can_lend, reason) = db.local_can_lend(amount)?;
            Ok(base_wire(local_peer_id.clone(), request.request_hash, "token_grant")
                .with_amount(Some(amount))
                .with_granted(can_lend)
                .with_reason(&reason))
        }
        "cancel_request" => Ok(base_wire(local_peer_id.clone(), request.request_hash, "ack")
            .with_granted(true)
            .with_reason("cancelled")),
        "proxy_call" => {
            let proxy_req = request
                .proxy_request
                .ok_or_else(|| anyhow!("proxy_call missing payload"))?;
            let proxy_res =
                execute_remote_proxy_call(db, http, vault_password, &request.requester_id, proxy_req).await;
            Ok(base_wire(local_peer_id.clone(), request.request_hash, "proxy_result")
                .with_granted(proxy_res.error.is_none())
                .with_reason_opt(proxy_res.error.clone())
                .with_proxy_response(Some(proxy_res)))
        }
        "leave_circle" => {
            db.set_peer_status(&peer.to_string(), false)?;
            db.insert_security_event("peer_left_circle", &format!("peer {} left circle", peer))?;
            let _ = db.rotate_circle_key();
            Ok(base_wire(local_peer_id.clone(), request.request_hash, "ack")
                .with_granted(true)
                .with_reason("peer removed"))
        }
        _ => Ok(base_wire(local_peer_id.clone(), request.request_hash, "ack")
            .with_granted(false)
            .with_reason("unsupported message")),
    }
}

async fn execute_remote_proxy_call(
    db: &Arc<Db>,
    http: &reqwest::Client,
    vault_password: &Arc<RwLock<Option<String>>>,
    requester_peer_id: &str,
    payload: ProxyRelayRequest,
) -> ProxyRelayResponse {
    if db.get_circle_key_plain().ok().flatten().is_none() {
        return ProxyRelayResponse {
            request_hash: payload.request_hash,
            status: 403,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body_b64: STANDARD.encode("{\"error\":\"circle key missing\"}"),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            request_id: None,
            error: Some("circle key missing".to_string()),
        };
    }

    let (can_lend, reason) = match db.local_can_lend(1000) {
        Ok(v) => v,
        Err(err) => {
            return ProxyRelayResponse {
                request_hash: payload.request_hash,
                status: 503,
                headers: vec![],
                body_b64: STANDARD.encode(format!("{{\"error\":\"{err}\"}}")),
                model: None,
                input_tokens: 0,
                output_tokens: 0,
                request_id: None,
                error: Some("local availability check failed".to_string()),
            }
        }
    };

    if !can_lend {
        return ProxyRelayResponse {
            request_hash: payload.request_hash,
            status: 429,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body_b64: STANDARD.encode(format!("{{\"error\":\"{reason}\"}}")),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            request_id: None,
            error: Some(reason),
        };
    }

    let provider = payload.provider.clone();
    let blocked_models = db
        .get_setting("blocked_model_patterns")
        .ok()
        .flatten()
        .unwrap_or_default();
    if !blocked_models.is_empty() {
        if let Ok(body) = STANDARD.decode(&payload.body_b64) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body) {
                if let Some(model) = v.get("model").and_then(|x| x.as_str()) {
                    if blocked_models
                        .split(',')
                        .any(|pattern| !pattern.trim().is_empty() && model.contains(pattern.trim()))
                    {
                        return ProxyRelayResponse {
                            request_hash: payload.request_hash,
                            status: 403,
                            headers: vec![("content-type".to_string(), "application/json".to_string())],
                            body_b64: STANDARD.encode("{\"error\":\"model blocked by policy\"}"),
                            model: Some(model.to_string()),
                            input_tokens: 0,
                            output_tokens: 0,
                            request_id: None,
                            error: Some("blocked by model policy".to_string()),
                        };
                    }
                }
            }
        }
    }
    let base_url = if provider == "openai" {
        "https://api.openai.com"
    } else {
        "https://api.anthropic.com"
    };
    let url = format!(
        "{base_url}{}{}",
        payload.path,
        payload
            .query
            .as_ref()
            .map(|q| format!("?{q}"))
            .unwrap_or_default()
    );

    let password = match vault_password.read().await.clone() {
        Some(v) => v,
        None => {
            return ProxyRelayResponse {
                request_hash: payload.request_hash,
                status: 401,
                headers: vec![],
                body_b64: STANDARD.encode("{\"error\":\"vault locked\"}"),
                model: None,
                input_tokens: 0,
                output_tokens: 0,
                request_id: None,
                error: Some("vault locked".to_string()),
            }
        }
    };

    let key = match db.get_provider_key(&provider) {
        Ok(Some(k)) => {
            let device_salt = match db.get_or_create_device_salt() {
                Ok(v) => v,
                Err(err) => {
                    return ProxyRelayResponse {
                        request_hash: payload.request_hash,
                        status: 500,
                        headers: vec![],
                        body_b64: STANDARD.encode(format!("{{\"error\":\"{err}\"}}")),
                        model: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        request_id: None,
                        error: Some("device salt unavailable".to_string()),
                    }
                }
            };
            match vault::decrypt_api_key(&k.encrypted_key, &password, &device_salt) {
            Ok(v) => v,
            Err(err) => {
                return ProxyRelayResponse {
                    request_hash: payload.request_hash,
                    status: 401,
                    headers: vec![],
                    body_b64: STANDARD.encode(format!("{{\"error\":\"{err}\"}}")),
                    model: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    request_id: None,
                    error: Some("key decrypt failed".to_string()),
                }
            }
        }
        }
        _ => {
            return ProxyRelayResponse {
                request_hash: payload.request_hash,
                status: 404,
                headers: vec![],
                body_b64: STANDARD.encode("{\"error\":\"no provider key\"}"),
                model: None,
                input_tokens: 0,
                output_tokens: 0,
                request_id: None,
                error: Some("no provider key".to_string()),
            }
        }
    };

    let mut req = http.request(
        payload
            .method
            .parse::<reqwest::Method>()
            .unwrap_or(reqwest::Method::POST),
        url,
    );

    for (k, v) in &payload.headers {
        let lk = k.to_ascii_lowercase();
        if lk == "host" || lk == "content-length" || lk == "x-api-key" || lk == "authorization" {
            continue;
        }
        req = req.header(k, v);
    }

    if provider == "openai" {
        req = req.header("authorization", format!("Bearer {key}"));
    } else {
        req = req.header("x-api-key", key);
    }

    let body = STANDARD.decode(&payload.body_b64).unwrap_or_default();
    let upstream = match req.body(body).send().await {
        Ok(v) => v,
        Err(err) => {
            return ProxyRelayResponse {
                request_hash: payload.request_hash,
                status: 502,
                headers: vec![],
                body_b64: STANDARD.encode(format!("{{\"error\":\"{err}\"}}")),
                model: None,
                input_tokens: 0,
                output_tokens: 0,
                request_id: None,
                error: Some("upstream request failed".to_string()),
            }
        }
    };

    let status = upstream.status().as_u16();
    let headers = upstream
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
        .collect::<Vec<_>>();
    let request_id = upstream
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());

    let bytes = upstream.bytes().await.unwrap_or_default().to_vec();
    let usage = usage_from_json_body(&bytes).unwrap_or_default();

    let _ = db.increment_local_daily_used(usage.input_tokens + usage.output_tokens);
    let _ = db.insert_transaction(&TransactionInsert {
        tx_type: "lent".to_string(),
        peer_id: Some(requester_peer_id.to_string()),
        provider,
        model: usage.model.clone(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        request_hash: payload.request_hash.clone(),
    });
    let _ = db.insert_audit_log(
        "inbound",
        Some(requester_peer_id),
        usage.model.as_deref(),
        usage.input_tokens,
        usage.output_tokens,
        &payload.request_hash,
    );

    ProxyRelayResponse {
        request_hash: payload.request_hash,
        status,
        headers,
        body_b64: STANDARD.encode(bytes),
        model: usage.model,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        request_id,
        error: None,
    }
}

fn signing_payload(requester_id: &str, request_hash: &str, nonce: &str, ts: i64) -> String {
    format!("{requester_id}|{request_hash}|{nonce}|{ts}")
}

fn base_wire(requester_id: String, request_hash: Option<String>, msg_type: &str) -> TokenWireMessage {
    TokenWireMessage {
        msg_type: msg_type.to_string(),
        request_hash,
        requester_id,
        amount: None,
        granted: None,
        reason: None,
        availability_state: None,
        daily_limit_tokens: None,
        daily_used_tokens: None,
        proxy_request: None,
        proxy_response: None,
        request_nonce: None,
        request_ts: None,
        signer_public_key_b64: None,
        signature_b64: None,
        circle_key_recipient: None,
        circle_key_box: None,
        ledger_entries: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LedgerEnvelopeBody {
    entry_id: String,
    tx_type: String,
    peer_id: Option<String>,
    provider: String,
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    request_hash: String,
}

fn build_ledger_envelope(
    db: &Db,
    signer_key: &identity::Keypair,
    local_peer_id: PeerId,
    tx: &TransactionInsert,
) -> Result<LedgerGossipRecord> {
    let circle_key = db.ensure_circle_key()?;
    let lamport = db.next_lamport()?;
    let body = LedgerEnvelopeBody {
        entry_id: Uuid::new_v4().to_string(),
        tx_type: tx.tx_type.clone(),
        peer_id: tx.peer_id.clone(),
        provider: tx.provider.clone(),
        model: tx.model.clone(),
        input_tokens: tx.input_tokens,
        output_tokens: tx.output_tokens,
        request_hash: tx.request_hash.clone(),
    };
    let body_json = serde_json::to_vec(&body)?;
    let nonce = Uuid::new_v4().to_string();
    let cipher_b64 = encrypt_with_circle_passphrase(&body_json, &circle_key)?;
    let payload = format!("{}|{}|{}|{}", body.entry_id, local_peer_id, lamport, cipher_b64);
    let sig = signer_key.sign(payload.as_bytes())?;
    Ok(LedgerGossipRecord {
        entry_id: body.entry_id,
        signer_peer_id: local_peer_id.to_string(),
        lamport_ts: lamport,
        cipher_b64,
        nonce,
        signature_b64: STANDARD.encode(sig),
        signer_pubkey_b64: STANDARD.encode(signer_key.public().encode_protobuf()),
        observed_ts: chrono::Utc::now().to_rfc3339(),
    })
}

fn apply_ledger_gossip(db: &Db, envelope: LedgerGossipRecord) -> Result<()> {
    // verify signature first
    let pub_bytes = STANDARD.decode(&envelope.signer_pubkey_b64)?;
    let public_key = identity::PublicKey::try_decode_protobuf(&pub_bytes)?;
    let expected = PeerId::from_public_key(&public_key).to_string();
    if expected != envelope.signer_peer_id {
        return Err(anyhow!("ledger signer mismatch"));
    }
    let payload = format!(
        "{}|{}|{}|{}",
        envelope.entry_id, envelope.signer_peer_id, envelope.lamport_ts, envelope.cipher_b64
    );
    let sig = STANDARD.decode(&envelope.signature_b64)?;
    if !public_key.verify(payload.as_bytes(), &sig) {
        return Err(anyhow!("invalid ledger signature"));
    }

    let changed = db.upsert_ledger_gossip(&envelope)?;
    if !changed {
        return Ok(());
    }
    let circle_key = db.ensure_circle_key()?;
    let plain = decrypt_with_circle_passphrase(&envelope.cipher_b64, &circle_key)?;
    let body: LedgerEnvelopeBody = serde_json::from_slice(&plain)?;
    db.observe_lamport(envelope.lamport_ts)?;
    db.insert_transaction(&TransactionInsert {
        tx_type: body.tx_type,
        peer_id: body.peer_id,
        provider: body.provider,
        model: body.model,
        input_tokens: body.input_tokens,
        output_tokens: body.output_tokens,
        request_hash: body.request_hash,
    })?;
    Ok(())
}

fn encrypt_with_circle_passphrase(plain: &[u8], circle_key: &str) -> Result<String> {
    let encryptor = age::Encryptor::with_user_passphrase(SecretString::from(circle_key.to_string()));
    let mut out = vec![];
    let mut writer = encryptor.wrap_output(&mut out)?;
    writer.write_all(plain)?;
    writer.finish()?;
    Ok(STANDARD.encode(out))
}

fn decrypt_with_circle_passphrase(cipher_b64: &str, circle_key: &str) -> Result<Vec<u8>> {
    let cipher = STANDARD.decode(cipher_b64)?;
    let decryptor = age::Decryptor::new(cipher.as_slice())?;
    let passphrase_identity = age::scrypt::Identity::new(SecretString::from(circle_key.to_string()));
    let mut reader = decryptor.decrypt(std::iter::once(&passphrase_identity as &dyn age::Identity))
        .map_err(|_| anyhow!("circle key decrypt failed"))?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut out)?;
    Ok(out)
}

fn encrypt_circle_key_for_recipient(circle_key: &str, recipient: &str) -> Result<String> {
    let recipient = age::x25519::Recipient::from_str(recipient)
        .map_err(|e| anyhow!("invalid circle key recipient: {e}"))?;
    let recipients: [&dyn age::Recipient; 1] = [&recipient];
    let encryptor = age::Encryptor::with_recipients(recipients.into_iter())?;
    let mut out = vec![];
    let mut writer = encryptor.wrap_output(&mut out)?;
    writer.write_all(circle_key.as_bytes())?;
    writer.finish()?;
    Ok(STANDARD.encode(out))
}

fn decrypt_circle_key_box(cipher_b64: &str, join_secret_identity: &str) -> Result<String> {
    let cipher = STANDARD.decode(cipher_b64)?;
    let decryptor = age::Decryptor::new(cipher.as_slice())?;
    let identity = age::x25519::Identity::from_str(join_secret_identity)
        .map_err(|e| anyhow!("invalid join identity: {e}"))?;
    let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_| anyhow!("circle key box decrypt failed"))?;
    let mut out = String::new();
    std::io::Read::read_to_string(&mut reader, &mut out)?;
    Ok(out)
}

fn endpoint_is_relay(endpoint: &libp2p::core::ConnectedPoint) -> bool {
    let addr = endpoint.get_remote_address();
    addr.iter().any(|p| matches!(p, Protocol::P2pCircuit))
}

fn validate_signed_token_request(request: &TokenWireMessage, peer: &PeerId) -> Result<()> {
    let nonce = request
        .request_nonce
        .as_ref()
        .ok_or_else(|| anyhow!("missing nonce"))?;
    let ts = request.request_ts.ok_or_else(|| anyhow!("missing timestamp"))?;
    let req_hash = request
        .request_hash
        .as_ref()
        .ok_or_else(|| anyhow!("missing request hash"))?;
    let pub_b64 = request
        .signer_public_key_b64
        .as_ref()
        .ok_or_else(|| anyhow!("missing signer key"))?;
    let sig_b64 = request
        .signature_b64
        .as_ref()
        .ok_or_else(|| anyhow!("missing signature"))?;

    let pub_bytes = STANDARD.decode(pub_b64).context("invalid signer key b64")?;
    let sig = STANDARD.decode(sig_b64).context("invalid signature b64")?;
    let public_key = identity::PublicKey::try_decode_protobuf(&pub_bytes).context("invalid protobuf public key")?;
    let expected_peer = PeerId::from_public_key(&public_key);
    if &expected_peer != peer {
        return Err(anyhow!("signer peer mismatch"));
    }
    if request.requester_id != peer.to_string() {
        return Err(anyhow!("requester id mismatch"));
    }
    let payload = signing_payload(&request.requester_id, req_hash, nonce, ts);
    if !public_key.verify(payload.as_bytes(), &sig) {
        return Err(anyhow!("signature verify failed"));
    }
    Ok(())
}

fn parse_invite(invite: &str) -> Result<InvitePayload> {
    let parsed = Url::parse(invite).context("invalid invite url")?;
    let data = parsed
        .query_pairs()
        .find_map(|(k, v)| if k == "data" { Some(v.into_owned()) } else { None })
        .ok_or_else(|| anyhow!("invite missing data payload"))?;

    let decoded = urlencoding::decode(&data)?.into_owned();
    let payload: InvitePayload = serde_json::from_str(&decoded)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_token_request_for(
        keypair: &identity::Keypair,
        request_hash: &str,
        nonce: &str,
        ts: i64,
    ) -> TokenWireMessage {
        let peer_id = PeerId::from(keypair.public()).to_string();
        let payload = signing_payload(&peer_id, request_hash, nonce, ts);
        let sig = keypair.sign(payload.as_bytes()).unwrap();
        TokenWireMessage {
            msg_type: "token_request".to_string(),
            request_hash: Some(request_hash.to_string()),
            requester_id: peer_id,
            amount: Some(1000),
            granted: None,
            reason: None,
            availability_state: None,
            daily_limit_tokens: None,
            daily_used_tokens: None,
            proxy_request: None,
            proxy_response: None,
            request_nonce: Some(nonce.to_string()),
            request_ts: Some(ts),
            signer_public_key_b64: Some(STANDARD.encode(keypair.public().encode_protobuf())),
            signature_b64: Some(STANDARD.encode(sig)),
            circle_key_recipient: None,
            circle_key_box: None,
            ledger_entries: None,
        }
    }

    #[test]
    fn invalid_signature_rejected() {
        let keypair = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(keypair.public());
        let mut req = signed_token_request_for(
            &keypair,
            "hash-1",
            "nonce-1",
            chrono::Utc::now().timestamp(),
        );
        req.signature_b64 = Some(STANDARD.encode("tampered"));
        let result = validate_signed_token_request(&req, &peer);
        assert!(result.is_err(), "tampered signature should be rejected");
    }

    #[test]
    fn key_never_in_message_verification() {
        let keypair = identity::Keypair::generate_ed25519();
        let req = signed_token_request_for(
            &keypair,
            "hash-2",
            "nonce-2",
            chrono::Utc::now().timestamp(),
        );
        let serialized = serde_json::to_string(&req).unwrap().to_lowercase();
        assert!(!serialized.contains("x-api-key"));
        assert!(!serialized.contains("authorization"));
        assert!(!serialized.contains("sk-"));
    }
}
