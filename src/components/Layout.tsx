import { ReactNode } from "react";
import Nav, { AppView } from "./Nav";

type Props = {
  current: AppView;
  onChange: (view: AppView) => void;
  children: ReactNode;
  circleName: string;
  displayName: string;
  sharingLabel: string;
  availability: string;
};

export default function Layout({
  current,
  onChange,
  children,
  circleName,
  displayName,
  sharingLabel,
  availability
}: Props) {
  return (
    <div className="app-shell">
      <header className="titlebar" data-tauri-drag-region>
        <div className="flex items-center gap-2 text-[11px]">
          <span className="h-2 w-2 rounded-full bg-[var(--accent)]" />
          <span className="mono tracking-[0.2em]">tokenunion</span>
        </div>
        <div className="mono text-[10px] text-[var(--muted)]">[{circleName}]</div>
      </header>

      <div className="grid h-[calc(100%-28px)] grid-cols-[160px_1fr]">
        <div className="relative">
          <Nav
            current={current}
            onChange={onChange}
            displayName={displayName}
            sharingLabel={sharingLabel}
            availability={availability}
          />
        </div>
        <main className="h-full p-4">{children}</main>
      </div>
    </div>
  );
}
