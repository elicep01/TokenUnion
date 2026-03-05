import { Fragment, MouseEvent, useEffect, useMemo, useState } from "react";
import { useAppStore } from "../stores/appStore";

const days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function parseBitmap(bitmap: string): boolean[] {
  if (!bitmap || bitmap.length !== 168) return Array.from({ length: 168 }, () => false);
  return bitmap.split("").map((c) => c === "1");
}

function toBitmap(cells: boolean[]): string {
  return cells.map((c) => (c ? "1" : "0")).join("");
}

export default function Schedule() {
  const { schedule, refreshSchedule, setSchedule } = useAppStore();
  const [cells, setCells] = useState<boolean[]>(Array.from({ length: 168 }, () => false));
  const [paintValue, setPaintValue] = useState<boolean | null>(null);

  useEffect(() => {
    void refreshSchedule();
  }, [refreshSchedule]);

  useEffect(() => {
    if (schedule) setCells(parseBitmap(schedule.weekly_active_bitmap));
  }, [schedule]);

  const activeCount = useMemo(() => cells.filter(Boolean).length, [cells]);

  const paint = (idx: number) => {
    setCells((prev) => {
      const next = [...prev];
      const target = paintValue ?? !next[idx];
      next[idx] = target;
      return next;
    });
  };

  const onDown = (idx: number) => {
    setPaintValue(!cells[idx]);
    paint(idx);
  };

  const save = async () => {
    const tz = schedule?.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    await setSchedule(tz, toBitmap(cells), schedule?.sharing_override || "auto");
  };

  const applyPreset = (startHour: number, endHour: number) => {
    setCells(() => {
      const next = Array.from({ length: 168 }, () => false);
      for (let day = 0; day < 7; day += 1) {
        for (let hour = startHour; hour < endHour; hour += 1) {
          next[day * 24 + hour] = true;
        }
      }
      return next;
    });
  };

  return (
    <div className="page-enter flex h-full flex-col gap-2">
      <p className="text-sm text-[var(--muted-strong)]">
        Your timezone: <span className="mono">{schedule?.timezone || "UTC"}</span> · change
      </p>

      <div className="flex flex-wrap gap-2">
        <button className="btn btn-ghost text-[11px]" onClick={() => applyPreset(9, 23)}>Preset: 9a-11p</button>
        <button className="btn btn-ghost text-[11px]" onClick={() => applyPreset(0, 24)}>Preset: always on</button>
        <button className="btn btn-ghost text-[11px]" onClick={() => setCells(Array.from({ length: 168 }, () => false))}>Clear all</button>
      </div>

      <div className="surface scroll-area p-3" onMouseLeave={() => setPaintValue(null)}>
        <div className="grid grid-cols-[40px_repeat(7,1fr)] gap-1">
          <div />
          {days.map((d) => (
            <p key={d} className="text-center text-[11px] font-light text-[var(--muted-strong)]">{d}</p>
          ))}

          {Array.from({ length: 24 }).map((_, hour) => {
            const label = hour % 2 === 0 ? `${(hour % 12 || 12).toString().toLowerCase()}${hour < 12 ? "a" : "p"}` : "";
            return (
              <Fragment key={`sch-hour-${hour}`}>
                <p key={`l-${hour}`} className="mono text-[10px] text-[var(--muted)]">{label}</p>
                {Array.from({ length: 7 }).map((__, day) => {
                  const idx = day * 24 + hour;
                  return (
                    <button
                      key={`${day}-${hour}`}
                      className="h-4 w-4 rounded-[3px]"
                      style={{ background: cells[idx] ? "rgba(108,180,255,0.3)" : "rgba(255,255,255,0.04)" }}
                      onMouseDown={(e: MouseEvent) => {
                        e.preventDefault();
                        onDown(idx);
                      }}
                      onMouseEnter={() => {
                        if (paintValue !== null) paint(idx);
                      }}
                      onMouseUp={() => setPaintValue(null)}
                    />
                  );
                })}
              </Fragment>
            );
          })}
        </div>
      </div>

      <div className="mono text-[11px] text-[var(--muted)]">
        Currently: {schedule?.sharing_override === "paused" ? "paused" : "sharing"} · {activeCount} active blocks/week
      </div>

      <button className="btn w-fit" onClick={() => void save()}>Save schedule</button>
    </div>
  );
}
