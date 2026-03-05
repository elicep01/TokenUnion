import { useEffect, useMemo, useState } from "react";
import { RadialBar, RadialBarChart, ResponsiveContainer } from "recharts";
import { useAppStore } from "../stores/appStore";

function useCountUp(target: number, duration = 600) {
  const [value, setValue] = useState(0);

  useEffect(() => {
    let frame = 0;
    const start = performance.now();

    const tick = (now: number) => {
      const progress = Math.min((now - start) / duration, 1);
      setValue(Math.round(target * progress));
      if (progress < 1) frame = requestAnimationFrame(tick);
    };

    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [target, duration]);

  return value;
}

function statusColor(state: string): string {
  if (state === "available") return "var(--online)";
  if (state === "limited") return "var(--warning)";
  return "var(--offline)";
}

export default function Dashboard() {
  const { stats, poolStatus, transactions, refreshDashboard, refreshPool, refreshLedger } = useAppStore();
  const [clockTick, setClockTick] = useState(Date.now());

  useEffect(() => {
    void refreshDashboard();
    void refreshPool();
    void refreshLedger();

    const slow = setInterval(() => {
      void refreshDashboard();
      void refreshPool();
      void refreshLedger();
    }, 5000);
    const fast = setInterval(() => setClockTick(Date.now()), 1000);
    return () => {
      clearInterval(slow);
      clearInterval(fast);
    };
  }, [refreshDashboard, refreshPool, refreshLedger]);

  const online = poolStatus.filter((m) => m.online).length;
  const total = Math.max(poolStatus.length, 1);
  const availabilityPct = Math.round((online / total) * 100);
  const contributed = transactions
    .filter((t) => t.tx_type === "lent")
    .reduce((sum, t) => sum + t.input_tokens + t.output_tokens, 0);
  const borrowed = transactions
    .filter((t) => t.tx_type === "borrowed")
    .reduce((sum, t) => sum + t.input_tokens + t.output_tokens, 0);
  const remainingEst = Math.max((stats?.total_tokens_today ?? 0) - borrowed, 0);

  const cContributed = useCountUp(contributed);
  const cBorrowed = useCountUp(borrowed);
  const cRemaining = useCountUp(remainingEst);

  const live = useMemo(() => transactions.slice(0, 10), [transactions]);

  return (
    <div className="page-enter grid h-full grid-rows-[1fr_1fr] gap-3">
      <section className="grid grid-cols-[290px_1fr] gap-3">
        <div className="surface p-3">
          <div className="h-[200px]">
            <ResponsiveContainer width="100%" height="100%">
              <RadialBarChart
                data={[{ name: "online", value: availabilityPct, fill: "var(--accent)" }]}
                innerRadius="65%"
                outerRadius="100%"
                startAngle={90}
                endAngle={-270}
              >
                <RadialBar dataKey="value" cornerRadius={6} background={{ fill: "rgba(255,255,255,0.06)" }} />
              </RadialBarChart>
            </ResponsiveContainer>
          </div>
          <div className="-mt-20 text-center">
            <p className="mono text-3xl">{online}/{total}</p>
            <p className="mono text-[11px] text-[var(--muted)]">online</p>
          </div>
          <div className="mt-8 grid grid-cols-3 gap-2">
            <p className="surface mono px-2 py-1 text-center text-[11px]">↑ {cContributed}</p>
            <p className="surface mono px-2 py-1 text-center text-[11px]">↓ {cBorrowed}</p>
            <p className="surface mono px-2 py-1 text-center text-[11px]">~{cRemaining}</p>
          </div>
        </div>

        <div className="surface p-3">
          <h3 className="mb-2 text-sm text-[var(--muted-strong)]">Circle members</h3>
          <div className="space-y-1.5">
            {poolStatus.slice(0, 6).map((member) => {
              const sleeping = member.availability_state === "sleeping" || !member.online;
              const balance = transactions
                .filter((t) => t.peer_id === member.peer_id)
                .reduce((sum, t) => sum + (t.tx_type === "lent" ? 1 : -1) * (t.input_tokens + t.output_tokens), 0);
              const time = new Date(clockTick).toLocaleTimeString([], {
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
                timeZone: member.timezone || "UTC"
              });

              return (
                <div key={member.peer_id} className="surface grid grid-cols-[10px_1fr_auto_auto] items-center gap-2 px-2 py-2" style={{ opacity: sleeping ? 0.4 : 1 }}>
                  <span className="h-2 w-2 rounded-full" style={{ background: statusColor(member.availability_state) }} />
                  <p className="truncate text-[12px]">{member.display_name}</p>
                  <p className="mono text-[11px] text-[var(--muted)]">{time}</p>
                  <p className="mono text-[11px]" style={{ color: balance >= 0 ? "var(--online)" : "var(--offline)" }}>
                    {balance >= 0 ? "+" : ""}
                    {balance}
                  </p>
                </div>
              );
            })}
            {poolStatus.length === 0 ? <p className="display-font text-center text-xl italic text-[var(--muted)]">No circle yet.</p> : null}
          </div>
        </div>
      </section>

      <section className="surface p-3">
        <h3 className="mb-2 text-sm text-[var(--muted-strong)]">Live feed</h3>
        <div className="scroll-area h-[220px] space-y-1 pr-1">
          {live.map((entry, idx) => {
            const borderColor = entry.tx_type === "borrowed" ? "var(--borrowed)" : entry.tx_type === "self" ? "var(--accent)" : "var(--online)";
            return (
              <div
                key={entry.id}
                className="surface mono grid grid-cols-[70px_1fr_140px_90px] items-center gap-2 px-2 py-2 text-[11px]"
                style={{ borderLeftColor: borderColor, borderLeftWidth: 2, animation: `pageEnter 0.3s ease ${idx * 0.1}s both` }}
              >
                <p>{new Date(entry.ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</p>
                <p className="truncate">{entry.peer_id || "you"}</p>
                <p className="truncate text-[var(--muted)]">{entry.model || "-"}</p>
                <p className="text-right">{entry.input_tokens + entry.output_tokens}</p>
              </div>
            );
          })}
          {transactions.length === 0 ? <div className="skeleton h-10 w-full" /> : null}
        </div>
      </section>
    </div>
  );
}
