import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import { parseDateValue, toDateValue } from "../lib/calendar";
import { applicationLabel, formatCompact, formatTokens, humanStatus, projectLabel } from "../lib/format";
import { dayStartIso, laneCount, layoutSegments } from "../lib/workTimeline";
import type { WorkTimelineDto } from "../types";
import { DatePicker } from "./ui/DatePicker";
import { EmptyState } from "./EmptyState";
import { KpiCard } from "./Kpi";
import { LoadingOverlay } from "./LoadingOverlay";

const AXIS_HOURS = [0, 3, 6, 9, 12, 15, 18, 21, 24];
const LANE_HEIGHT = 34;

function shiftDay(day: string, delta: number): string {
  const date = parseDateValue(day) ?? new Date();
  date.setDate(date.getDate() + delta);
  return toDateValue(date);
}

export function WorkTimeline() {
  const [day, setDay] = useState(() => toDateValue(new Date()));
  const [data, setData] = useState<WorkTimelineDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const generationRef = useRef(0);

  useEffect(() => {
    const generation = ++generationRef.current;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 发起请求前先置 loading
    setLoading(true);
    setError(null);
    invoke<WorkTimelineDto>("get_work_timeline", { day })
      .then((next) => {
        if (generation === generationRef.current) {
          setData(next);
        }
      })
      .catch((err: unknown) => {
        if (generation === generationRef.current) {
          setError(humanStatus(err));
        }
      })
      .finally(() => {
        if (generation === generationRef.current) {
          setLoading(false);
        }
      });
  }, [day]);

  const segments = data?.segments ?? [];
  const layout = layoutSegments(segments, dayStartIso(day));
  const lanes = Math.max(1, laneCount(layout));

  return (
    <div className="stack worktime">
      <section className="panel worktime-head">
        <div className="worktime-day-nav">
          <button
            type="button"
            className="date-nav-btn"
            onClick={() => setDay((current) => shiftDay(current, -1))}
            aria-label="前一天"
          >
            <Icon name="chevron" size={13} />
          </button>
          <DatePicker ariaLabel="选择日期" value={day} onChange={setDay} />
          <button
            type="button"
            className="date-nav-btn"
            onClick={() => setDay((current) => shiftDay(current, 1))}
            aria-label="后一天"
          >
            <Icon name="chevron" size={13} className="flip" />
          </button>
        </div>
        <div className="kpi-row worktime-kpis">
          <KpiCard
            icon="tokens"
            tone="purple"
            label="当日 Token 总消耗"
            value={formatCompact(data?.total_tokens ?? 0)}
          />
          <KpiCard
            icon="sessions"
            tone="cyan"
            label="工作片段数"
            value={formatTokens(data?.segment_count ?? 0)}
          />
        </div>
      </section>

      <LoadingOverlay active={loading} className="panel worktime-timeline">
        {error ? (
          <EmptyState icon="alertTriangle" tone="warn" title="加载失败" hint={error} />
        ) : segments.length === 0 && !loading ? (
          <EmptyState icon="clock" title="这天没有工作记录" hint="换一天试试，或检查数据源是否已同步" />
        ) : (
          <div className="worktime-axis-wrap">
            <div className="worktime-axis">
              {AXIS_HOURS.map((hour) => (
                <span key={hour} style={{ left: `${(hour / 24) * 100}%` }}>
                  {String(hour).padStart(2, "0")}:00
                </span>
              ))}
            </div>
            <div
              className="worktime-lanes"
              style={{ height: lanes * LANE_HEIGHT }}
            >
              {AXIS_HOURS.slice(1, -1).map((hour) => (
                <div key={hour} className="worktime-gridline" style={{ left: `${(hour / 24) * 100}%` }} />
              ))}
              {layout.map((item) => {
                const left = (item.startMinutes / 1440) * 100;
                const width = Math.max(0, ((item.endMinutes - item.startMinutes) / 1440) * 100);
                const label = `${projectLabel(item.segment.project)} · ${applicationLabel(
                  item.segment.source,
                )}/${item.segment.model}`;
                return (
                  <div
                    key={`${item.segment.source}:${item.segment.session_id}`}
                    className="worktime-segment"
                    title={`${label}\n${formatTokens(item.segment.total_tokens)} tokens`}
                    style={{
                      left: `${left}%`,
                      width: `${width}%`,
                      top: item.lane * LANE_HEIGHT,
                    }}
                  >
                    <span>{label}</span>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </LoadingOverlay>
    </div>
  );
}
