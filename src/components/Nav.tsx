import { ReactNode } from "react";

export type AppView = "dashboard" | "circle" | "ledger" | "vault" | "schedule" | "settings";

type Props = {
  current: AppView;
  onChange: (view: AppView) => void;
  displayName: string;
  sharingLabel: string;
  availability: string;
};

const items: { id: AppView; label: string }[] = [
  { id: "dashboard", label: "Dashboard" },
  { id: "circle", label: "Circle" },
  { id: "ledger", label: "Ledger" },
  { id: "vault", label: "Vault" },
  { id: "schedule", label: "Schedule" },
  { id: "settings", label: "Settings" }
];

function statusColor(availability: string): string {
  if (availability === "available") return "var(--online)";
  if (availability === "limited" || availability === "sleeping") return "var(--warning)";
  return "var(--offline)";
}

export default function Nav({ current, onChange, displayName, sharingLabel, availability }: Props) {
  return (
    <aside className="h-full w-40 border-r border-[var(--border)] px-3 py-3">
      <div className="mb-6 flex items-center gap-2">
        <div className="h-3 w-3 rounded-full border border-[var(--border)] bg-[var(--surface)]" />
        <p className="mono text-[12px] tracking-[0.3em] text-[var(--muted-strong)]">tokenunion</p>
      </div>

      <nav className="space-y-0.5">
        {items.map((item) => {
          const active = item.id === current;
          return (
            <button
              key={item.id}
              onClick={() => onChange(item.id)}
              className="flex w-full items-center border-l-2 px-2 py-1.5 text-left text-[12px]"
              style={{
                borderLeftColor: active ? "var(--accent)" : "transparent",
                color: active ? "var(--text)" : "var(--muted)"
              }}
              onMouseEnter={(e) => {
                if (!active) e.currentTarget.style.color = "var(--muted-strong)";
              }}
              onMouseLeave={(e) => {
                if (!active) e.currentTarget.style.color = "var(--muted)";
              }}
            >
              {item.label}
            </button>
          );
        })}
      </nav>

      <div className="absolute bottom-3 left-3 right-3">
        <div className="surface px-2 py-2">
          <div className="mb-1 flex items-center gap-2">
            <span className="h-2 w-2 rounded-full" style={{ background: statusColor(availability) }} />
            <p className="truncate text-[12px] text-[var(--text)]">{displayName}</p>
          </div>
          <p className="mono text-[10px] uppercase text-[var(--muted)]">{sharingLabel}</p>
        </div>
      </div>
    </aside>
  );
}
