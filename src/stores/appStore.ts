import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type DashboardStats = {
  total_tokens_today: number;
  requests_today: number;
  active_key_label: string | null;
  proxy_running: boolean;
  proxy_port: number;
};

export type VaultKey = {
  id: number;
  provider: string;
  label: string;
  masked_key: string;
  created_at: string;
};

export type LocalNode = {
  peer_id: string;
  display_name: string;
  timezone: string;
  availability_state: string;
};

export type Peer = {
  peer_id: string;
  display_name: string;
  timezone: string;
  multiaddr: string | null;
  online: boolean;
  last_seen: string | null;
  availability_state: string;
  daily_limit_tokens: number;
  daily_used_tokens: number;
};

export type PeerMessage = {
  id: number;
  ts: string;
  direction: "inbound" | "outbound";
  peer_id: string;
  msg_type: string;
  amount: number;
  granted: boolean | null;
  reason: string | null;
};

export type ScheduleConfig = {
  timezone: string;
  weekly_active_bitmap: string;
  sharing_override: "auto" | "share_now" | "paused";
};

export type PoolStatus = {
  peer_id: string;
  display_name: string;
  availability_state: string;
  online: boolean;
  timezone: string;
  daily_limit_tokens: number;
  daily_used_tokens: number;
};

export type Transaction = {
  id: number;
  ts: string;
  tx_type: "self" | "lent" | "borrowed";
  peer_id: string | null;
  provider: string;
  model: string | null;
  input_tokens: number;
  output_tokens: number;
  request_hash: string;
};

export type FairUse = {
  peer_id: string;
  contributed_tokens: number;
  consumed_tokens: number;
  balance_tokens: number;
};

export type AuditLog = {
  id: number;
  ts: string;
  direction: "inbound" | "outbound";
  peer_id: string | null;
  model: string | null;
  input_tokens: number;
  output_tokens: number;
  request_nonce: string;
};

export type SecurityEvent = {
  id: number;
  ts: string;
  event_type: string;
  detail: string;
};

export type RateLimitStat = {
  peer_id: string;
  max_requests_per_min: number;
  current_window_count: number;
};

export type AppPreferences = {
  auto_start: boolean;
  notifications_enabled: boolean;
  appearance: string;
  proxy_port: number;
};

export type OAuthSession = {
  provider: string;
  account_label: string | null;
  scopes: string | null;
  expires_at: string | null;
  created_at: string;
  updated_at: string;
};

export type AnthropicOAuthConfig = {
  client_id: string;
  authorize_url: string;
  token_url: string;
  redirect_uri: string;
  scopes: string;
};

export type OAuthAuthorizePayload = {
  authorize_url: string;
  state: string;
  code_verifier: string;
};

type AppState = {
  stats: DashboardStats | null;
  keys: VaultKey[];
  localNode: LocalNode | null;
  peers: Peer[];
  peerMessages: PeerMessage[];
  schedule: ScheduleConfig | null;
  poolStatus: PoolStatus[];
  transactions: Transaction[];
  fairUse: FairUse[];
  auditLog: AuditLog[];
  securityEvents: SecurityEvent[];
  rateLimitStats: RateLimitStat[];
  onboardingCompleted: boolean;
  preferences: AppPreferences | null;
  relayMode: "off" | "self_hosted" | "community";
  oauthSessions: OAuthSession[];
  anthropicOAuthConfig: AnthropicOAuthConfig | null;
  loading: boolean;
  error: string | null;
  refreshDashboard: () => Promise<void>;
  refreshKeys: () => Promise<void>;
  refreshPeers: () => Promise<void>;
  refreshPeerMessages: () => Promise<void>;
  refreshSchedule: () => Promise<void>;
  refreshPool: () => Promise<void>;
  refreshLedger: () => Promise<void>;
  refreshSecurity: () => Promise<void>;
  loadOnboarding: () => Promise<void>;
  completeOnboarding: () => Promise<void>;
  refreshPreferences: () => Promise<void>;
  refreshRelayMode: () => Promise<void>;
  refreshOAuthSessions: () => Promise<void>;
  refreshAnthropicOAuthConfig: () => Promise<void>;
  setProviderKey: (provider: string, label: string, apiKey: string, password: string) => Promise<void>;
  deleteKey: (id: number) => Promise<void>;
  unlockVault: (password: string) => Promise<void>;
  startProxy: () => Promise<void>;
  stopProxy: () => Promise<void>;
  updateProfile: (displayName: string, timezone: string) => Promise<void>;
  createInviteUrl: () => Promise<string>;
  joinInvite: (inviteUrl: string) => Promise<void>;
  setSchedule: (timezone: string, weeklyActiveBitmap: string, sharingOverride: string) => Promise<void>;
  tickSchedule: () => Promise<void>;
  setPeerRateLimit: (peerId: string, maxPerMin: number) => Promise<void>;
  leaveCircle: () => Promise<void>;
  setContentFilterPatterns: (patternsCsv: string) => Promise<void>;
  setAppPreferences: (autoStart: boolean, notificationsEnabled: boolean, appearance: string) => Promise<void>;
  setProxyPort: (port: number) => Promise<void>;
  exportLedgerCsv: () => Promise<string>;
  checkForUpdates: () => Promise<boolean>;
  setSharingOverride: (sharingOverride: string) => Promise<void>;
  setRelayMode: (relayMode: "off" | "self_hosted" | "community") => Promise<void>;
  setAnthropicOAuthConfig: (config: AnthropicOAuthConfig) => Promise<void>;
  createAnthropicOAuthAuthorizeUrl: () => Promise<OAuthAuthorizePayload>;
  exchangeAnthropicOAuthCode: (
    code: string,
    returnedState: string,
    codeVerifier: string,
    expectedState: string,
    accountLabel?: string
  ) => Promise<void>;
  deleteOAuthSession: (provider: string) => Promise<void>;
};

export const useAppStore = create<AppState>((set, get) => ({
  stats: null,
  keys: [],
  localNode: null,
  peers: [],
  peerMessages: [],
  schedule: null,
  poolStatus: [],
  transactions: [],
  fairUse: [],
  auditLog: [],
  securityEvents: [],
  rateLimitStats: [],
  onboardingCompleted: false,
  preferences: null,
  relayMode: "self_hosted",
  oauthSessions: [],
  anthropicOAuthConfig: null,
  loading: false,
  error: null,

  refreshDashboard: async () => {
    set({ loading: true, error: null });
    try {
      const stats = await invoke<DashboardStats>("get_dashboard_stats");
      set({ stats });
    } catch (error) {
      set({ error: String(error) });
    } finally {
      set({ loading: false });
    }
  },

  refreshKeys: async () => {
    try {
      const keys = await invoke<VaultKey[]>("list_keys");
      set({ keys, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshPeers: async () => {
    try {
      const [localNode, peers] = await Promise.all([
        invoke<LocalNode>("get_local_node"),
        invoke<Peer[]>("list_peers")
      ]);
      set({ localNode, peers, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshPeerMessages: async () => {
    try {
      const peerMessages = await invoke<PeerMessage[]>("list_token_messages");
      set({ peerMessages, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshSchedule: async () => {
    try {
      const schedule = await invoke<ScheduleConfig>("get_schedule_config");
      set({ schedule, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshPool: async () => {
    try {
      const poolStatus = await invoke<PoolStatus[]>("get_pool_status");
      set({ poolStatus, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshLedger: async () => {
    try {
      const [transactions, fairUse] = await Promise.all([
        invoke<Transaction[]>("get_ledger_transactions"),
        invoke<FairUse[]>("get_fair_use_7d")
      ]);
      set({ transactions, fairUse, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshSecurity: async () => {
    try {
      const [auditLog, securityEvents, rateLimitStats] = await Promise.all([
        invoke<AuditLog[]>("get_audit_log"),
        invoke<SecurityEvent[]>("get_security_events"),
        invoke<RateLimitStat[]>("get_rate_limit_stats")
      ]);
      set({ auditLog, securityEvents, rateLimitStats, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  loadOnboarding: async () => {
    try {
      const onboardingCompleted = await invoke<boolean>("get_onboarding_completed");
      set({ onboardingCompleted, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  completeOnboarding: async () => {
    await invoke("complete_onboarding");
    set({ onboardingCompleted: true });
  },

  refreshPreferences: async () => {
    try {
      const preferences = await invoke<AppPreferences>("get_app_preferences");
      set({ preferences, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshRelayMode: async () => {
    try {
      const relayMode = await invoke<"off" | "self_hosted" | "community">("get_relay_mode");
      set({ relayMode, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshOAuthSessions: async () => {
    try {
      const oauthSessions = await invoke<OAuthSession[]>("list_oauth_sessions");
      set({ oauthSessions, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  refreshAnthropicOAuthConfig: async () => {
    try {
      const anthropicOAuthConfig = await invoke<AnthropicOAuthConfig>("get_anthropic_oauth_config");
      set({ anthropicOAuthConfig, error: null });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  setProviderKey: async (provider, label, apiKey, password) => {
    await invoke("set_provider_key", { provider, label, api_key: apiKey, password });
    await get().refreshKeys();
    await get().refreshDashboard();
  },

  deleteKey: async (id) => {
    await invoke("delete_key", { id });
    await get().refreshKeys();
    await get().refreshDashboard();
  },

  unlockVault: async (password) => {
    await invoke("unlock_vault", { password });
    await get().refreshDashboard();
  },

  startProxy: async () => {
    await invoke("start_proxy");
    await get().refreshDashboard();
  },

  stopProxy: async () => {
    await invoke("stop_proxy");
    await get().refreshDashboard();
  },

  updateProfile: async (displayName, timezone) => {
    await invoke("update_profile", { display_name: displayName, timezone });
    await get().refreshPeers();
  },

  createInviteUrl: async () => invoke<string>("create_invite_url"),

  joinInvite: async (inviteUrl) => {
    await invoke("join_invite", { invite_url: inviteUrl });
    await get().refreshPeers();
  },

  setSchedule: async (timezone, weeklyActiveBitmap, sharingOverride) => {
    await invoke("set_schedule_config", {
      timezone,
      weekly_active_bitmap: weeklyActiveBitmap,
      sharing_override: sharingOverride
    });
    await get().refreshSchedule();
    await get().refreshPeers();
    await get().refreshPool();
  },

  tickSchedule: async () => {
    await invoke("evaluate_schedule_tick");
    await get().refreshPool();
    await get().refreshPeers();
  },

  setPeerRateLimit: async (peerId, maxPerMin) => {
    await invoke("set_peer_rate_limit", { peer_id: peerId, max_per_min: maxPerMin });
    await get().refreshSecurity();
  },

  leaveCircle: async () => {
    await invoke("leave_circle");
    await get().refreshPeers();
    await get().refreshSecurity();
  },

  setContentFilterPatterns: async (patternsCsv) => {
    await invoke("set_content_filter_patterns", { blocked_model_patterns: patternsCsv });
  },

  setAppPreferences: async (autoStart, notificationsEnabled, appearance) => {
    await invoke("set_app_preferences", {
      auto_start: autoStart,
      notifications_enabled: notificationsEnabled,
      appearance
    });
    await get().refreshPreferences();
  },

  setProxyPort: async (port) => {
    await invoke("set_proxy_port", { port });
    await get().refreshDashboard();
    await get().refreshPreferences();
  },

  exportLedgerCsv: async () => {
    return invoke<string>("export_ledger_csv");
  },

  checkForUpdates: async () => {
    return invoke<boolean>("check_for_updates");
  },

  setSharingOverride: async (sharingOverride) => {
    await invoke("set_sharing_override", { sharing_override: sharingOverride });
    await get().refreshPool();
    await get().refreshPeers();
  },

  setRelayMode: async (relayMode) => {
    await invoke("set_relay_mode", { relay_mode: relayMode });
    set({ relayMode });
  },

  setAnthropicOAuthConfig: async (config) => {
    await invoke("set_anthropic_oauth_config", {
      client_id: config.client_id,
      authorize_url: config.authorize_url,
      token_url: config.token_url,
      redirect_uri: config.redirect_uri,
      scopes: config.scopes
    });
    await get().refreshAnthropicOAuthConfig();
  },

  createAnthropicOAuthAuthorizeUrl: async () => {
    const cfg = get().anthropicOAuthConfig ?? (await invoke<AnthropicOAuthConfig>("get_anthropic_oauth_config"));
    return invoke<OAuthAuthorizePayload>("create_anthropic_oauth_authorize_url", {
      client_id: cfg.client_id,
      redirect_uri: cfg.redirect_uri,
      scopes: cfg.scopes,
      authorize_base_url: cfg.authorize_url
    });
  },

  exchangeAnthropicOAuthCode: async (code, returnedState, codeVerifier, expectedState, accountLabel) => {
    const cfg = get().anthropicOAuthConfig ?? (await invoke<AnthropicOAuthConfig>("get_anthropic_oauth_config"));
    await invoke("exchange_anthropic_oauth_code", {
      token_url: cfg.token_url,
      client_id: cfg.client_id,
      redirect_uri: cfg.redirect_uri,
      code,
      code_verifier: codeVerifier,
      expected_state: expectedState,
      returned_state: returnedState,
      account_label: accountLabel ?? null
    });
    await get().refreshOAuthSessions();
  },

  deleteOAuthSession: async (provider) => {
    await invoke("delete_oauth_session", { provider });
    await get().refreshOAuthSessions();
  }
}));
