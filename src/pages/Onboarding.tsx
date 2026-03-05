import { Fragment, MouseEvent, useMemo, useState } from "react";
import { useAppStore } from "../stores/appStore";

type Mode = "create" | "join";

const steps = ["Welcome", "Identity", "Keys", "Circle", "Schedule", "Done"];
const days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function defaultSchedule(): boolean[] {
  const cells = Array.from({ length: 7 * 24 }, () => false);
  for (let d = 0; d < 7; d += 1) {
    for (let h = 9; h < 23; h += 1) {
      cells[d * 24 + h] = true;
    }
  }
  return cells;
}

function toBitmap(cells: boolean[]): string {
  return cells.map((c) => (c ? "1" : "0")).join("");
}

export default function Onboarding() {
  const {
    updateProfile,
    setProviderKey,
    createInviteUrl,
    joinInvite,
    setSchedule,
    completeOnboarding,
    refreshPeers,
    peers,
    keys
  } = useAppStore();

  const [step, setStep] = useState(0);
  const [mode, setMode] = useState<Mode>("create");
  const [name, setName] = useState("operator");
  const [timezone, setTimezone] = useState(Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC");
  const [vaultPassword, setVaultPassword] = useState("");
  const [anthropic, setAnthropic] = useState("");
  const [openai, setOpenai] = useState("");
  const [inviteUrl, setInviteUrl] = useState("");
  const [joinUrl, setJoinUrl] = useState("");
  const [joinError, setJoinError] = useState("");
  const [stepError, setStepError] = useState("");
  const [schedule, setScheduleCells] = useState<boolean[]>(() => defaultSchedule());
  const [paintValue, setPaintValue] = useState<boolean | null>(null);

  const validInvite = useMemo(() => joinUrl.includes("invite") || joinUrl.startsWith("tokenunion://"), [joinUrl]);

  const saveIdentity = async () => {
    setStepError("");
    localStorage.setItem("tokenunion_onboarding_name", name);
    localStorage.setItem("tokenunion_onboarding_timezone", timezone);
    try {
      await updateProfile(name, timezone);
    } catch {
      // Allow onboarding to continue if backend init lags behind UI.
      setStepError("Profile sync is still starting. Continuing with local values.");
    }
    setStep(2);
  };

  const saveKeys = async () => {
    setStepError("");
    if ((anthropic || openai) && !vaultPassword) return;
    try {
      if (anthropic) await setProviderKey("anthropic", "Anthropic", anthropic, vaultPassword);
      if (openai) await setProviderKey("openai", "OpenAI", openai, vaultPassword);
    } catch {
      setStepError("Could not save keys right now. You can add them later in Vault.");
    }
    setStep(3);
  };

  const handleCreateInvite = async () => {
    const url = await createInviteUrl();
    setInviteUrl(url);
  };

  const handleJoin = async () => {
    setJoinError("");
    if (!validInvite) {
      setJoinError("Invalid invite format");
      return;
    }
    try {
      await joinInvite(joinUrl);
      await refreshPeers();
      setStep(4);
    } catch {
      setJoinError("Could not connect with invite");
    }
  };

  const finishSchedule = async () => {
    setStepError("");
    try {
      await setSchedule(timezone, toBitmap(schedule), "auto");
    } catch {
      setStepError("Schedule sync is delayed. You can update it later from Schedule.");
    }
    setStep(5);
  };

  const paintCell = (idx: number) => {
    setScheduleCells((prev) => {
      const next = [...prev];
      const target = paintValue ?? !next[idx];
      next[idx] = target;
      return next;
    });
  };

  const finish = async () => {
    await completeOnboarding();
  };

  return (
    <div className="h-full w-full page-enter">
      <div className="mx-auto flex h-full max-w-3xl flex-col items-center justify-center px-6">
        <div className="mb-6 flex gap-2">
          {steps.map((_, i) => (
            <span
              key={i}
              className="h-2.5 w-2.5 rounded-full border"
              style={{
                background: i < step ? "var(--accent)" : "transparent",
                borderColor: i === step ? "var(--accent)" : "var(--border)"
              }}
            />
          ))}
        </div>

        <div className="surface w-full max-w-xl p-6">
          {step === 0 ? (
            <div className="space-y-5 text-center">
              <h1 className="display-font text-4xl italic">Your circle awaits.</h1>
              <p className="mx-auto max-w-md text-sm font-light text-[var(--muted-strong)]">
                TokenUnion pools API credits across your friend group. Timezone-aware. Private. Local.
              </p>
              <div className="flex justify-center gap-3">
                <button className="btn px-4 py-2" onClick={() => { setMode("create"); setStep(1); }}>
                  Create a circle
                </button>
                <button className="btn btn-ghost px-4 py-2" onClick={() => { setMode("join"); setStep(1); }}>
                  Join a circle
                </button>
              </div>
            </div>
          ) : null}

          {step === 1 ? (
            <div className="space-y-4 text-center">
              <p className="text-sm text-[var(--muted-strong)]">What should your circle call you?</p>
              <input className="input-base mx-auto block w-full max-w-md text-center text-lg" value={name} onChange={(e) => setName(e.target.value)} />
              <select className="input-base mx-auto block w-full max-w-md" value={timezone} onChange={(e) => setTimezone(e.target.value)}>
                <option value={timezone}>{timezone}</option>
                <option value="UTC">UTC</option>
                <option value="America/Chicago">America/Chicago</option>
                <option value="Asia/Kolkata">Asia/Kolkata</option>
              </select>
              <button className="btn" onClick={() => void saveIdentity()}>Continue →</button>
            </div>
          ) : null}

          {step === 2 ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="h-2 w-2 rounded-full" style={{ background: "var(--accent)" }} />
                <p className="text-sm">Anthropic</p>
                <input className="input-base ml-auto w-64" value={anthropic} onChange={(e) => setAnthropic(e.target.value)} placeholder="sk-ant-..." />
              </div>
              <div className="flex items-center gap-2">
                <span className="h-2 w-2 rounded-full" style={{ background: "var(--online)" }} />
                <p className="text-sm">OpenAI/Codex</p>
                <input className="input-base ml-auto w-64" value={openai} onChange={(e) => setOpenai(e.target.value)} placeholder="sk-proj-..." />
              </div>
              <input className="input-base w-full" type="password" value={vaultPassword} onChange={(e) => setVaultPassword(e.target.value)} placeholder="Vault password (required if adding keys)" />
              <p className="mono text-[11px] text-[var(--muted)]">keys never leave this device</p>
              <div className="flex items-center justify-between">
                <button className="text-xs text-[var(--muted)]" onClick={() => setStep(3)}>skip for now</button>
                <button className="btn" onClick={() => void saveKeys()}>Add</button>
              </div>
            </div>
          ) : null}

          {step === 3 && mode === "create" ? (
            <div className="space-y-4">
              <button className="btn" onClick={() => void handleCreateInvite()}>Generate invite</button>
              <div className="surface p-3">
                <p className="mono break-all text-xs">{inviteUrl || "tokenunion://invite/..."}</p>
              </div>
              {inviteUrl ? (
                <img
                  src={`https://api.qrserver.com/v1/create-qr-code/?size=220x220&data=${encodeURIComponent(inviteUrl)}`}
                  alt="Invite QR"
                  className="h-28 w-28 rounded border border-[var(--border)] bg-white p-1"
                  onError={(e) => {
                    const target = e.currentTarget as HTMLImageElement;
                    target.style.display = "none";
                    setStepError("QR service unavailable. Share the invite link directly.");
                  }}
                />
              ) : (
                <div className="flex h-28 w-28 items-center justify-center rounded border border-dashed border-[var(--border)] text-sm text-[var(--muted)]">QR</div>
              )}
              <p className="text-xs text-[var(--muted-strong)]">Share this with friends. They'll use it to join.</p>
              <button className="btn" onClick={() => setStep(4)}>I'm done inviting →</button>
            </div>
          ) : null}

          {step === 3 && mode === "join" ? (
            <div className="space-y-4">
              <input
                className="input-base w-full"
                value={joinUrl}
                onChange={(e) => setJoinUrl(e.target.value)}
                placeholder="Paste your invite link"
              />
              <p className="text-xs" style={{ color: validInvite ? "var(--online)" : "var(--offline)" }}>
                {validInvite ? "Invite format valid" : joinError || "Waiting for valid invite"}
              </p>
              <button className="btn" disabled={!validInvite} onClick={() => void handleJoin()}>Connect →</button>
            </div>
          ) : null}

          {step === 4 ? (
            <div className="space-y-3">
              <p className="text-sm">When are you okay sharing credits?</p>
              <div className="grid grid-cols-[28px_repeat(7,12px)] gap-1" onMouseLeave={() => setPaintValue(null)}>
                <div />
                {days.map((d) => (
                  <p key={d} className="text-center text-[9px] text-[var(--muted)]">{d[0]}</p>
                ))}
                {Array.from({ length: 24 }).map((_, hour) => (
                  <Fragment key={`onb-hour-${hour}`}>
                    <p key={`h-${hour}`} className="mono text-[9px] text-[var(--muted)]">{hour}</p>
                    {Array.from({ length: 7 }).map((__, day) => {
                      const idx = day * 24 + hour;
                      return (
                        <button
                          key={`${day}-${hour}`}
                          className="h-[12px] w-[12px] rounded-[2px]"
                          style={{ background: schedule[idx] ? "color-mix(in srgb, var(--accent) 30%, transparent)" : "rgba(255,255,255,0.04)" }}
                          onMouseDown={(e: MouseEvent) => {
                            e.preventDefault();
                            setPaintValue(!schedule[idx]);
                            paintCell(idx);
                          }}
                          onMouseEnter={() => {
                            if (paintValue !== null) paintCell(idx);
                          }}
                          onMouseUp={() => setPaintValue(null)}
                        />
                      );
                    })}
                  </Fragment>
                ))}
              </div>
              <p className="text-xs text-[var(--muted)]">{timezone} · We'll automatically offer your credits during these hours.</p>
              <button className="btn" onClick={() => void finishSchedule()}>Continue →</button>
            </div>
          ) : null}

          {step === 5 ? (
            <div className="space-y-4 text-center">
              <h2 className="display-font text-4xl italic">You're in.</h2>
              <p className="text-sm text-[var(--muted-strong)]">
                {name} · {peers.length + 1} members · {keys.length > 0 ? `${keys.length} key(s)` : "keys skipped"}
              </p>
              <button className="btn" onClick={() => void finish()}>Open TokenUnion →</button>
            </div>
          ) : null}

          {stepError ? <p className="mt-3 text-center text-xs" style={{ color: "var(--warning)" }}>{stepError}</p> : null}
        </div>
      </div>
    </div>
  );
}
