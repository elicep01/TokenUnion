import { useEffect, useMemo, useState } from "react";
import { useAppStore } from "../stores/appStore";

function parseOAuthCallback(input: string): { code: string; state: string } | null {
  try {
    const parsed = new URL(input.trim());
    const code = parsed.searchParams.get("code");
    const state = parsed.searchParams.get("state");
    if (!code || !state) return null;
    return { code, state };
  } catch {
    return null;
  }
}

export default function Vault() {
  const {
    keys,
    refreshKeys,
    setProviderKey,
    deleteKey,
    oauthSessions,
    anthropicOAuthConfig,
    refreshOAuthSessions,
    refreshAnthropicOAuthConfig,
    setAnthropicOAuthConfig,
    createAnthropicOAuthAuthorizeUrl,
    exchangeAnthropicOAuthCode,
    deleteOAuthSession
  } = useAppStore();

  const [open, setOpen] = useState(false);
  const [provider, setProvider] = useState("anthropic");
  const [apiKey, setApiKey] = useState("");
  const [label, setLabel] = useState("");
  const [password, setPassword] = useState("");

  const [oauthClientId, setOauthClientId] = useState("");
  const [oauthAuthorizeUrl, setOauthAuthorizeUrl] = useState("https://claude.ai/oauth/authorize");
  const [oauthTokenUrl, setOauthTokenUrl] = useState("");
  const [oauthRedirectUri, setOauthRedirectUri] = useState("http://127.0.0.1:8787/oauth/callback");
  const [oauthScopes, setOauthScopes] = useState("user:profile user:inference user:sessions:claude_code user:mcp_servers");

  const [oauthExpectedState, setOauthExpectedState] = useState("");
  const [oauthVerifier, setOauthVerifier] = useState("");
  const [callbackUrl, setCallbackUrl] = useState("");
  const [oauthError, setOauthError] = useState("");

  useEffect(() => {
    void refreshKeys();
    void refreshOAuthSessions();
    void refreshAnthropicOAuthConfig();
  }, [refreshKeys, refreshOAuthSessions, refreshAnthropicOAuthConfig]);

  useEffect(() => {
    if (!anthropicOAuthConfig) return;
    setOauthClientId(anthropicOAuthConfig.client_id || "");
    setOauthAuthorizeUrl(anthropicOAuthConfig.authorize_url || "https://claude.ai/oauth/authorize");
    setOauthTokenUrl(anthropicOAuthConfig.token_url || "");
    setOauthRedirectUri(anthropicOAuthConfig.redirect_uri || "http://127.0.0.1:8787/oauth/callback");
    setOauthScopes(anthropicOAuthConfig.scopes || "user:profile user:inference user:sessions:claude_code user:mcp_servers");
  }, [anthropicOAuthConfig]);

  const addKey = async () => {
    if (!apiKey || !password) return;
    await setProviderKey(provider, label || provider, apiKey, password);
    setApiKey("");
    setLabel("");
    setPassword("");
    setOpen(false);
  };

  const anthropicSession = useMemo(
    () => oauthSessions.find((s) => s.provider === "anthropic") ?? null,
    [oauthSessions]
  );

  const saveOAuthConfig = async () => {
    await setAnthropicOAuthConfig({
      client_id: oauthClientId,
      authorize_url: oauthAuthorizeUrl,
      token_url: oauthTokenUrl,
      redirect_uri: oauthRedirectUri,
      scopes: oauthScopes
    });
  };

  const startOAuth = async () => {
    setOauthError("");
    if (!oauthClientId || !oauthTokenUrl || !oauthRedirectUri) {
      setOauthError("Set client_id, token_url, and redirect_uri first.");
      return;
    }
    await saveOAuthConfig();
    const payload = await createAnthropicOAuthAuthorizeUrl();
    setOauthExpectedState(payload.state);
    setOauthVerifier(payload.code_verifier);
    window.open(payload.authorize_url, "_blank", "noopener,noreferrer");
  };

  const finishOAuth = async () => {
    setOauthError("");
    const parsed = parseOAuthCallback(callbackUrl);
    if (!parsed) {
      setOauthError("Paste full callback URL with code and state.");
      return;
    }
    try {
      await exchangeAnthropicOAuthCode(parsed.code, parsed.state, oauthVerifier, oauthExpectedState, "anthropic-oauth");
      setCallbackUrl("");
      setOauthExpectedState("");
      setOauthVerifier("");
    } catch (e) {
      setOauthError(String(e));
    }
  };

  return (
    <div className="page-enter scroll-area flex h-full flex-col gap-2 pr-1">
      <div className="flex items-center justify-between">
        <h2 className="display-font text-3xl">Vault</h2>
        <button className="btn" onClick={() => setOpen((v) => !v)}>{open ? "Close" : "Add key"}</button>
      </div>

      <div className="surface p-3">
        <div className="mb-2 flex items-center justify-between">
          <p className="text-sm">Anthropic OAuth (PKCE)</p>
          {anthropicSession ? (
            <span className="mono rounded px-1.5 py-0.5 text-[10px]" style={{ background: "rgba(74,222,128,0.12)", color: "var(--online)" }}>
              connected
            </span>
          ) : (
            <span className="mono rounded px-1.5 py-0.5 text-[10px]" style={{ background: "rgba(248,113,113,0.12)", color: "var(--offline)" }}>
              not connected
            </span>
          )}
        </div>
        <div className="grid grid-cols-2 gap-2 text-[12px]">
          <input className="input-base" placeholder="client_id" value={oauthClientId} onChange={(e) => setOauthClientId(e.target.value)} />
          <input className="input-base" placeholder="token_url" value={oauthTokenUrl} onChange={(e) => setOauthTokenUrl(e.target.value)} />
          <input className="input-base col-span-2" placeholder="authorize_url" value={oauthAuthorizeUrl} onChange={(e) => setOauthAuthorizeUrl(e.target.value)} />
          <input className="input-base col-span-2" placeholder="redirect_uri" value={oauthRedirectUri} onChange={(e) => setOauthRedirectUri(e.target.value)} />
          <input className="input-base col-span-2" placeholder="scopes" value={oauthScopes} onChange={(e) => setOauthScopes(e.target.value)} />
        </div>
        <div className="mt-2 flex flex-wrap gap-2">
          <button className="btn" onClick={() => void saveOAuthConfig()}>Save OAuth config</button>
          <button className="btn" onClick={() => void startOAuth()}>Sign in with Anthropic</button>
          {anthropicSession ? (
            <button className="btn btn-destructive" onClick={() => void deleteOAuthSession("anthropic")}>Disconnect OAuth</button>
          ) : null}
        </div>
        {oauthVerifier ? (
          <div className="mt-2 space-y-2">
            <input
              className="input-base w-full"
              placeholder="Paste OAuth callback URL"
              value={callbackUrl}
              onChange={(e) => setCallbackUrl(e.target.value)}
            />
            <button className="btn" onClick={() => void finishOAuth()}>Complete OAuth</button>
          </div>
        ) : null}
        {oauthError ? <p className="mt-2 text-xs" style={{ color: "var(--warning)" }}>{oauthError}</p> : null}
      </div>

      {open ? (
        <div className="surface grid grid-cols-[140px_1fr_1fr_1fr_auto] items-center gap-2 p-3">
          <select className="input-base" value={provider} onChange={(e) => setProvider(e.target.value)}>
            <option value="anthropic">Anthropic</option>
            <option value="openai">OpenAI/Codex</option>
          </select>
          <input className="input-base" placeholder="API key" value={apiKey} onChange={(e) => setApiKey(e.target.value)} />
          <input className="input-base" placeholder="Label" value={label} onChange={(e) => setLabel(e.target.value)} />
          <input className="input-base" type="password" placeholder="Password" value={password} onChange={(e) => setPassword(e.target.value)} />
          <button className="btn" onClick={() => void addKey()}>Save</button>
        </div>
      ) : null}

      <div className="space-y-2">
        {keys.map((key) => (
          <div key={key.id} className="surface grid grid-cols-[1fr_170px_100px_auto_auto] items-center gap-2 p-3 text-[12px]">
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 rounded-full" style={{ background: key.provider === "openai" ? "var(--online)" : "var(--accent)" }} />
              <p>{key.provider}</p>
              <span className="mono rounded px-1.5 py-0.5 text-[10px]" style={{ background: "rgba(255,255,255,0.07)" }}>{key.label}</span>
            </div>
            <p className="mono text-[11px] text-[var(--muted)]">{key.masked_key}</p>
            <span className="mono rounded px-1.5 py-0.5 text-[10px]" style={{ background: "rgba(74,222,128,0.12)", color: "var(--online)" }}>
              active
            </span>
            <button className="btn btn-ghost" onClick={() => void navigator.clipboard.writeText(key.masked_key)}>Copy</button>
            <button className="btn btn-destructive" onClick={() => void deleteKey(key.id)}>Delete</button>
          </div>
        ))}
        {keys.length === 0 ? <p className="display-font p-8 text-center text-xl italic text-[var(--muted)]">No keys yet.</p> : null}
      </div>

      <p className="mono text-[10px] text-[var(--muted)]">encrypted at rest · never transmitted · device-bound</p>
    </div>
  );
}
