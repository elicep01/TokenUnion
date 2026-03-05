import { PoolStatus, DashboardStats, Transaction } from "../stores/appStore";

type Props = {
  circleName: string;
  availability: string;
  poolStatus: PoolStatus[];
  stats: DashboardStats | null;
  transactions: Transaction[];
  onToggleSharing: () => void;
  onOpen: () => void;
  onQuit: () => void;
};

function availabilityColor(state: string): string {
  if (state === "available") return "var(--online)";
  if (state === "limited" || state === "sleeping") return "var(--warning)";
  return "var(--offline)";
}

export default function TrayPopover({
  circleName,
  availability,
  poolStatus,
  stats,
  transactions,
  onToggleSharing,
  onOpen,
  onQuit
}: Props) {
  const online = poolStatus.filter((p) => p.online).length;
  const total = Math.max(poolStatus.length, 1);
  const contributed = transactions
    .filter((t) => t.tx_type === "lent")
    .reduce((sum, t) => sum + t.input_tokens + t.output_tokens, 0);

  return (
    <div className="surface h-[200px] w-[280px] p-3 text-[12px] page-enter">
      <div className="flex items-center justify-between">
        <p className="truncate text-[var(--text)]">{circleName}</p>
        <p className="mono flex items-center gap-1 text-[10px] text-[var(--muted)]">
          <span className="h-2 w-2 rounded-full" style={{ background: availabilityColor(availability) }} />
          {availability === "paused" ? "paused" : "sharing"}
        </p>
      </div>

      <div className="mt-2 flex items-center gap-3">
        <div className="surface flex h-12 w-12 items-center justify-center rounded-full">
          <span className="mono text-[11px]">{online}/{total}</span>
        </div>
        <p className="mono text-[11px] text-[var(--muted)]">online</p>
      </div>

      <div className="mt-2 grid grid-cols-2 gap-2">
        <div className="surface px-2 py-1">
          <p className="mono text-[10px] text-[var(--muted)]">used today</p>
          <p className="mono text-[12px]">{stats?.total_tokens_today ?? 0}</p>
        </div>
        <div className="surface px-2 py-1">
          <p className="mono text-[10px] text-[var(--muted)]">contributed</p>
          <p className="mono text-[12px]">{contributed}</p>
        </div>
      </div>

      <div className="my-2 h-px bg-[var(--border)]" />

      <button className="mb-1 block text-left text-[12px] text-[var(--text)]" onClick={onToggleSharing}>
        Pause sharing
      </button>
      <button className="mb-1 block text-left text-[12px] text-[var(--text)]" onClick={onOpen}>
        Open TokenUnion
      </button>
      <button className="block text-left text-[11px] text-[var(--muted)]" onClick={onQuit}>
        Quit
      </button>
    </div>
  );
}
