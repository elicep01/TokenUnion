import { useEffect, useMemo, useState } from "react";
import { useAppStore } from "../stores/appStore";

type Filter = "all" | "borrowed" | "lent" | "self";

export default function Ledger() {
  const { transactions, refreshLedger, exportLedgerCsv } = useAppStore();
  const [filter, setFilter] = useState<Filter>("all");

  useEffect(() => {
    void refreshLedger();
  }, [refreshLedger]);

  const rows = useMemo(() => {
    if (filter === "all") return transactions;
    return transactions.filter((t) => t.tx_type === filter);
  }, [transactions, filter]);

  const lent = transactions.filter((t) => t.tx_type === "lent").reduce((s, t) => s + t.input_tokens + t.output_tokens, 0);
  const borrowed = transactions.filter((t) => t.tx_type === "borrowed").reduce((s, t) => s + t.input_tokens + t.output_tokens, 0);
  const net = lent - borrowed;

  const exportCsv = async () => {
    const csv = await exportLedgerCsv();
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "tokenunion-ledger.csv";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="page-enter flex h-full flex-col gap-2">
      <div className="flex items-center justify-between">
        <div className="flex gap-3 text-[12px]">
          {(["all", "borrowed", "lent", "self"] as Filter[]).map((tab) => (
            <button
              key={tab}
              className="pb-1"
              style={{
                color: tab === filter ? "var(--text)" : "var(--muted)",
                borderBottom: tab === filter ? "1px solid var(--accent)" : "1px solid transparent"
              }}
              onClick={() => setFilter(tab)}
            >
              {tab[0].toUpperCase() + tab.slice(1)}
            </button>
          ))}
        </div>
        <button className="btn btn-ghost text-[11px]" onClick={() => void exportCsv()}>Export CSV</button>
      </div>

      <div className="surface flex-1 overflow-hidden">
        <div className="grid grid-cols-[110px_90px_1fr_140px_90px_90px] border-b border-[var(--border)] px-2 py-2 text-[10px] uppercase text-[var(--muted)]">
          <p>Time</p>
          <p>Direction</p>
          <p>Peer</p>
          <p>Model</p>
          <p>Tokens In</p>
          <p>Tokens Out</p>
        </div>
        <div className="scroll-area h-[calc(100%-64px)]">
          {rows.map((r) => {
            const symbol = r.tx_type === "lent" ? "↑" : r.tx_type === "borrowed" ? "↓" : "·";
            const color = r.tx_type === "lent" ? "var(--online)" : r.tx_type === "borrowed" ? "var(--borrowed)" : "var(--muted)";
            return (
              <div key={r.id} className="grid grid-cols-[110px_90px_1fr_140px_90px_90px] items-center border-b border-[var(--border)] px-2 py-2 text-[11px]">
                <p className="mono">{new Date(r.ts).toLocaleTimeString()}</p>
                <p className="mono" style={{ color }}>{symbol} {r.tx_type}</p>
                <p className="truncate">{r.peer_id || "you"}</p>
                <p className="truncate mono text-[var(--muted)]">{r.model || "-"}</p>
                <p className="mono">{r.input_tokens}</p>
                <p className="mono">{r.output_tokens}</p>
              </div>
            );
          })}
          {rows.length === 0 ? <p className="display-font p-8 text-center text-xl italic text-[var(--muted)]">No ledger entries yet.</p> : null}
        </div>
      </div>

      <div className="mono text-[11px] text-[var(--muted-strong)]">
        You contributed {lent} tokens, borrowed {borrowed} tokens. Net: <span style={{ color: net >= 0 ? "var(--online)" : "var(--offline)" }}>{net >= 0 ? "+" : ""}{net}</span>
      </div>
    </div>
  );
}
