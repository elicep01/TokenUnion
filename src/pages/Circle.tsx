import { useEffect, useMemo, useState } from "react";
import { useAppStore } from "../stores/appStore";

function initialColor(seed: string): string {
  const colors = ["var(--accent)", "var(--online)", "var(--warning)", "var(--borrowed)"];
  let total = 0;
  for (let i = 0; i < seed.length; i += 1) total += seed.charCodeAt(i);
  return colors[total % colors.length];
}

export default function Circle() {
  const { peers, fairUse, refreshPeers, refreshLedger, createInviteUrl, leaveCircle } = useAppStore();
  const [circleName, setCircleName] = useState(localStorage.getItem("tokenunion_circle_name") || "midnight union");
  const [editing, setEditing] = useState(false);
  const [inviteOpen, setInviteOpen] = useState(false);
  const [inviteLink, setInviteLink] = useState("");

  useEffect(() => {
    void refreshPeers();
    void refreshLedger();
  }, [refreshPeers, refreshLedger]);

  const balances = useMemo(() => {
    const map = new Map<string, number>();
    fairUse.forEach((f) => map.set(f.peer_id, f.balance_tokens));
    return map;
  }, [fairUse]);

  const openInvite = async () => {
    setInviteOpen(true);
    const link = await createInviteUrl();
    setInviteLink(link);
  };

  return (
    <div className="page-enter flex h-full flex-col gap-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {editing ? (
            <input
              className="input-base mono text-sm"
              value={circleName}
              onChange={(e) => setCircleName(e.target.value)}
              onBlur={() => {
                setEditing(false);
                localStorage.setItem("tokenunion_circle_name", circleName);
              }}
            />
          ) : (
            <h2 className="display-font text-3xl" onClick={() => setEditing(true)}>{circleName}</h2>
          )}
          <p className="mono text-xs text-[var(--muted)]">{peers.length + 1} members</p>
        </div>
        <button className="btn" onClick={() => void openInvite()}>Invite a new member</button>
      </div>

      <div className="scroll-area surface flex-1 space-y-2 p-2">
        {peers.map((peer) => {
          const bal = balances.get(peer.peer_id) ?? 0;
          return (
            <div key={peer.peer_id} className="surface grid grid-cols-[28px_1fr_85px_95px_110px_90px_70px] items-center gap-2 px-2 py-2 text-[11px]">
              <div className="flex h-7 w-7 items-center justify-center rounded-full text-[11px]" style={{ background: initialColor(peer.display_name) }}>
                {peer.display_name.charAt(0).toUpperCase()}
              </div>
              <div>
                <p className="text-[12px]">{peer.display_name}</p>
                <p className="mono text-[10px] text-[var(--muted)]">{peer.peer_id.slice(0, 8)}</p>
              </div>
              <p className="mono text-[10px] text-[var(--muted)]">{peer.timezone || "UTC"}</p>
              <p className="mono text-[10px] text-[var(--muted)]">{peer.last_seen ? new Date(peer.last_seen).toLocaleDateString() : "-"}</p>
              <p className="mono text-[10px]" style={{ color: bal >= 0 ? "var(--online)" : "var(--offline)" }}>
                {bal >= 0 ? "+" : ""}
                {bal}
              </p>
              <button
                className="btn btn-destructive"
                onClick={() => {
                  const ok = window.confirm(`Remove ${peer.display_name}?`);
                  if (ok) window.alert("Member removal wiring is pending in backend commands.");
                }}
              >
                Remove
              </button>
            </div>
          );
        })}
        {peers.length === 0 ? <p className="display-font text-center text-xl italic text-[var(--muted)]">Invite someone to start.</p> : null}
      </div>

      <button className="btn btn-destructive w-fit text-xs opacity-70 hover:opacity-100" onClick={() => void leaveCircle()}>
        Leave circle
      </button>

      {inviteOpen ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 backdrop-blur-[8px]">
          <div className="surface w-full max-w-[480px] p-4" style={{ boxShadow: "0 8px 32px rgba(0,0,0,0.4)" }}>
            <h3 className="mb-2 text-sm">Invite link</h3>
            <div className="surface mono break-all p-2 text-xs">{inviteLink || "loading..."}</div>
            {inviteLink ? (
              <img
                src={`https://api.qrserver.com/v1/create-qr-code/?size=220x220&data=${encodeURIComponent(inviteLink)}`}
                alt="Invite QR"
                className="mt-3 h-28 w-28 rounded border border-[var(--border)] bg-white p-1"
              />
            ) : (
              <div className="mt-3 flex h-28 w-28 items-center justify-center rounded border border-dashed border-[var(--border)] text-sm text-[var(--muted)]">QR</div>
            )}
            <button className="btn mt-4" onClick={() => setInviteOpen(false)}>Close</button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
