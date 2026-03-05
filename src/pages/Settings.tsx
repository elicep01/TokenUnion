import { ReactNode, useEffect, useState } from "react";
import { useAppStore } from "../stores/appStore";

function Row({ label, control }: { label: string; control: ReactNode }) {
  return (
    <div className="grid grid-cols-[220px_1fr] items-center border-b border-[var(--border)] py-2 text-[12px]">
      <p className="text-[var(--muted-strong)]">{label}</p>
      <div>{control}</div>
    </div>
  );
}

export default function Settings() {
  const {
    preferences,
    relayMode,
    refreshPreferences,
    refreshRelayMode,
    setAppPreferences,
    setProxyPort,
    setRelayMode,
    leaveCircle,
    checkForUpdates
  } = useAppStore();

  const [port, setPort] = useState(47821);
  const [autoStart, setAutoStart] = useState(false);
  const [relay, setRelay] = useState(localStorage.getItem("tokenunion_relay") || "/dns4/community-relay/tcp/4001/p2p/...");
  const [dailyLimit, setDailyLimit] = useState(localStorage.getItem("tokenunion_daily_limit") || "100");
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    void refreshPreferences();
    void refreshRelayMode();
  }, [refreshPreferences, refreshRelayMode]);

  useEffect(() => {
    if (!preferences) return;
    setPort(preferences.proxy_port);
    setAutoStart(preferences.auto_start);
  }, [preferences]);

  const persist = async () => {
    await setProxyPort(port);
    await setAppPreferences(autoStart, preferences?.notifications_enabled ?? true, "dark");
    localStorage.setItem("tokenunion_relay", relay);
    localStorage.setItem("tokenunion_daily_limit", dailyLimit);
  };

  return (
    <div className="page-enter scroll-area h-full pr-1">
      <p className="mono text-[10px] uppercase text-[var(--muted)]">PROXY</p>
      <Row
        label="Port"
        control={<input className="input-base w-24" type="number" value={port} onChange={(e) => setPort(Number(e.target.value))} />}
      />
      <Row
        label="Auto-start on login"
        control={
          <button className="btn" onClick={() => setAutoStart((v) => !v)}>{autoStart ? "Enabled" : "Disabled"}</button>
        }
      />

      <p className="mono mt-4 text-[10px] uppercase text-[var(--muted)]">APPEARANCE</p>
      <Row
        label="Theme"
        control={<button className="btn btn-ghost" disabled>Dark (Light coming soon)</button>}
      />

      <p className="mono mt-4 text-[10px] uppercase text-[var(--muted)]">CIRCLE</p>
      <Row
        label="Relay server"
        control={<input className="input-base w-full max-w-md" value={relay} onChange={(e) => setRelay(e.target.value)} />}
      />
      <Row
        label="Relay mode"
        control={
          <select
            className="input-base w-44"
            value={relayMode}
            onChange={(e) => void setRelayMode(e.target.value as "off" | "self_hosted" | "community")}
          >
            <option value="off">off</option>
            <option value="self_hosted">self_hosted</option>
            <option value="community">community</option>
          </select>
        }
      />
      <Row
        label="Daily share limit"
        control={
          <div className="flex items-center gap-2">
            <input className="input-base w-24" value={dailyLimit} onChange={(e) => setDailyLimit(e.target.value)} />
            <span className="mono text-[11px] text-[var(--muted)]">k tokens</span>
          </div>
        }
      />

      <div className="mt-3 flex gap-2">
        <button className="btn" onClick={() => void persist()}>Save settings</button>
        <button
          className="btn btn-ghost"
          onClick={async () => {
            setChecking(true);
            const has = await checkForUpdates();
            setChecking(false);
            window.alert(has ? "Update available" : "No updates found");
          }}
        >
          {checking ? "Checking..." : "Check updates"}
        </button>
      </div>

      <p className="mono mt-5 text-[10px] uppercase text-[var(--muted)]">DANGER ZONE</p>
      <div className="mt-1 flex gap-2">
        <button className="btn btn-destructive" onClick={() => void leaveCircle()}>Leave circle</button>
        <button
          className="btn btn-destructive"
          onClick={() => {
            const ok = window.confirm("Reset all local app data? This action is irreversible.");
            if (!ok) return;
            const second = window.confirm("Confirm reset again.");
            if (!second) return;
            localStorage.clear();
            window.location.reload();
          }}
        >
          Reset all data
        </button>
      </div>
    </div>
  );
}
