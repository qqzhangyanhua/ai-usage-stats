import { invoke } from "@tauri-apps/api/core";
import ReactECharts from "echarts-for-react";
import { useEffect, useMemo, useState } from "react";
import type {
  CodeVolumeSummary,
  Filter,
  FilterOptions,
  IngestReport,
  NamedAmount,
  OverviewDto,
  PriceEntry,
  PriceTable,
  SeriesPoint,
  SessionRow,
  TurnRow,
} from "./types";

type View =
  | "overview"
  | "trend"
  | "source"
  | "model"
  | "provider"
  | "project"
  | "sessions"
  | "cursor"
  | "settings";

const emptyFilter: Filter = {
  from: null,
  to: null,
  sources: [],
  models: [],
  projects: [],
};

function formatTokens(n: number): string {
  return n.toLocaleString("zh-CN");
}

function formatCost(n: number | null, unpriced: boolean): string {
  if (unpriced && n == null) {
    return "—";
  }
  if (n == null) {
    return "0";
  }
  return n.toFixed(4);
}

function providerChannel(name: string): string {
  const official = [
    "official",
    "anthropic",
    "openai",
    "google",
    "gemini",
    "xai",
    "grok",
    "codex_local_access",
    "deepseek-official",
  ];
  if (!name || name === "（未标注）") {
    return "未标注";
  }
  return official.includes(name) ? "官方" : "中转";
}

function rangeFromPreset(preset: string): { from: string | null; to: string | null } {
  if (preset === "7" || preset === "30") {
    const days = Number(preset);
    const to = new Date();
    const from = new Date(to.getTime() - days * 24 * 3600 * 1000);
    return { from: from.toISOString(), to: to.toISOString() };
  }
  return { from: null, to: null };
}

export default function App() {
  const [view, setView] = useState<View>("overview");
  const [filter, setFilter] = useState<Filter>(emptyFilter);
  const [preset, setPreset] = useState("all");
  const [options, setOptions] = useState<FilterOptions>({
    sources: [],
    models: [],
    projects: [],
  });
  const [overview, setOverview] = useState<OverviewDto | null>(null);
  const [trend, setTrend] = useState<SeriesPoint[]>([]);
  const [grain, setGrain] = useState("day");
  const [breakdown, setBreakdown] = useState<NamedAmount[]>([]);
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const [turns, setTurns] = useState<TurnRow[]>([]);
  const [selectedSession, setSelectedSession] = useState<{
    id: string;
    source: string;
  } | null>(null);
  const [prices, setPrices] = useState<PriceTable>({ prices: [] });
  const [codeVolume, setCodeVolume] = useState<CodeVolumeSummary | null>(null);
  const [status, setStatus] = useState("尚未摄取");
  const [busy, setBusy] = useState(false);

  async function refreshViews(nextFilter = filter) {
    const [nextOverview, nextOptions] = await Promise.all([
      invoke<OverviewDto>("get_overview", { filter: nextFilter }),
      invoke<FilterOptions>("get_filter_options"),
    ]);
    setOverview(nextOverview);
    setOptions(nextOptions);
    if (view === "trend") {
      setTrend(await invoke<SeriesPoint[]>("get_trend", { filter: nextFilter, grain }));
    }
    if (["source", "model", "provider", "project"].includes(view)) {
      const dimension = view;
      setBreakdown(
        await invoke<NamedAmount[]>("get_breakdown", {
          query: { filter: nextFilter, dimension },
        }),
      );
    }
    if (view === "sessions") {
      setSessions(await invoke<SessionRow[]>("get_top_sessions", { filter: nextFilter, limit: 30 }));
      if (selectedSession) {
        setTurns(
          await invoke<TurnRow[]>("get_session_turns", {
            sessionId: selectedSession.id,
            source: selectedSession.source,
            filter: nextFilter,
          }),
        );
      }
    }
    if (view === "cursor") {
      setCodeVolume(await invoke<CodeVolumeSummary>("get_code_volume"));
    }
    if (view === "settings") {
      setPrices(await invoke<PriceTable>("get_prices"));
    }
  }

  async function runIngest(label: string) {
    setBusy(true);
    setStatus(`${label}中…`);
    try {
      const report = await invoke<IngestReport>("ingest");
      setStatus(
        `${label}完成：解析 ${report.files_parsed}，跳过 ${report.files_skipped}，写入 ${report.records_written}`,
      );
      await refreshViews();
    } catch (error) {
      setStatus(`${label}失败：${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    invoke<string>("ping")
      .then(() => runIngest("启动摄取"))
      .catch(() => setStatus("IPC 未连通"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    refreshViews().catch((error) => setStatus(String(error)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, grain, selectedSession]);

  const nav: { id: View; label: string }[] = [
    { id: "overview", label: "总览" },
    { id: "trend", label: "时间趋势" },
    { id: "source", label: "按来源" },
    { id: "model", label: "按模型" },
    { id: "provider", label: "按 provider" },
    { id: "project", label: "按项目" },
    { id: "sessions", label: "Top 会话" },
    { id: "cursor", label: "Cursor 代码量" },
    { id: "settings", label: "设置" },
  ];

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>本机 AI 用量统计</h1>
        {nav.map((item) => (
          <button
            key={item.id}
            className={view === item.id ? "nav-btn active" : "nav-btn"}
            onClick={() => setView(item.id)}
          >
            {item.label}
          </button>
        ))}
        <button className="refresh-btn" disabled={busy} onClick={() => runIngest("刷新")}>
          刷新
        </button>
        <div className="status">{status}</div>
      </aside>
      <main className="main">
        {view !== "cursor" && view !== "settings" && (
          <Filters
            filter={filter}
            preset={preset}
            options={options}
            onPreset={(next) => {
              setPreset(next);
              const range = rangeFromPreset(next);
              const nextFilter = { ...filter, ...range };
              setFilter(nextFilter);
              refreshViews(nextFilter).catch((error) => setStatus(String(error)));
            }}
            onChange={(next) => {
              setFilter(next);
              refreshViews(next).catch((error) => setStatus(String(error)));
            }}
          />
        )}
        {view === "overview" && overview && <Overview overview={overview} />}
        {view === "trend" && <Trend grain={grain} setGrain={setGrain} points={trend} />}
        {["source", "model", "provider", "project"].includes(view) && (
          <Breakdown
            title={nav.find((n) => n.id === view)?.label ?? ""}
            rows={breakdown}
            showProviderChannel={view === "provider"}
          />
        )}
        {view === "sessions" && (
          <Sessions
            rows={sessions}
            turns={turns}
            selected={selectedSession}
            onSelect={setSelectedSession}
          />
        )}
        {view === "cursor" && <CursorPanel summary={codeVolume} />}
        {view === "settings" && (
          <Settings
            prices={prices}
            onChange={setPrices}
            onSave={async () => {
              await invoke("save_price_table", { prices });
              setStatus("单价已保存");
            }}
          />
        )}
      </main>
    </div>
  );
}

function Filters({
  filter,
  preset,
  options,
  onPreset,
  onChange,
}: {
  filter: Filter;
  preset: string;
  options: FilterOptions;
  onPreset: (preset: string) => void;
  onChange: (filter: Filter) => void;
}) {
  return (
    <div className="filters">
      <label>
        时间范围
        <select value={preset} onChange={(e) => onPreset(e.target.value)}>
          <option value="all">全部历史</option>
          <option value="7">近 7 天</option>
          <option value="30">近 30 天</option>
        </select>
      </label>
      <label>
        来源
        <select
          value={filter.sources[0] ?? ""}
          onChange={(e) =>
            onChange({ ...filter, sources: e.target.value ? [e.target.value] : [] })
          }
        >
          <option value="">全部</option>
          {options.sources.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </label>
      <label>
        模型
        <select
          value={filter.models[0] ?? ""}
          onChange={(e) =>
            onChange({ ...filter, models: e.target.value ? [e.target.value] : [] })
          }
        >
          <option value="">全部</option>
          {options.models.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </label>
      <label>
        项目
        <select
          value={filter.projects[0] ?? ""}
          onChange={(e) =>
            onChange({ ...filter, projects: e.target.value ? [e.target.value] : [] })
          }
        >
          <option value="">全部</option>
          {options.projects.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

function Overview({ overview }: { overview: OverviewDto }) {
  const cards = [
    ["总 token", formatTokens(overview.total_tokens)],
    ["输入", formatTokens(overview.input_tokens)],
    ["输出", formatTokens(overview.output_tokens)],
    ["缓存读", formatTokens(overview.cache_read_tokens)],
    ["缓存写", formatTokens(overview.cache_creation_tokens)],
    ["推理", formatTokens(overview.reasoning_tokens)],
    ["会话数", formatTokens(overview.session_count)],
    ["费用", formatCost(overview.cost, overview.unpriced)],
  ];
  return (
    <>
      <div className="cards">
        {cards.map(([label, value]) => (
          <div className="card" key={label}>
            <div className="label">{label}</div>
            <div className="value">{value}</div>
          </div>
        ))}
      </div>
      {overview.unpriced && <div className="note">部分模型单价未配置</div>}
    </>
  );
}

function Trend({
  grain,
  setGrain,
  points,
}: {
  grain: string;
  setGrain: (grain: string) => void;
  points: SeriesPoint[];
}) {
  const option = useMemo(
    () => ({
      tooltip: { trigger: "axis" },
      xAxis: { type: "category", data: points.map((p) => p.bucket) },
      yAxis: { type: "value" },
      series: [{ type: "bar", data: points.map((p) => p.total_tokens), name: "token" }],
    }),
    [points],
  );
  return (
    <>
      <div className="section-title">
        <h2>时间趋势</h2>
        <select value={grain} onChange={(e) => setGrain(e.target.value)}>
          <option value="day">按天</option>
          <option value="week">按周</option>
        </select>
      </div>
      <ReactECharts option={option} style={{ height: 360 }} />
    </>
  );
}

function Breakdown({
  title,
  rows,
  showProviderChannel,
}: {
  title: string;
  rows: NamedAmount[];
  showProviderChannel?: boolean;
}) {
  const option = useMemo(
    () => ({
      tooltip: { trigger: "axis" },
      xAxis: { type: "value" },
      yAxis: {
        type: "category",
        data: rows
          .map((r) =>
            showProviderChannel ? `${r.name}（${providerChannel(r.name)}）` : r.name,
          )
          .reverse(),
      },
      series: [{ type: "bar", data: rows.map((r) => r.total_tokens).reverse() }],
    }),
    [rows, showProviderChannel],
  );
  return (
    <>
      <div className="section-title">
        <h2>{title}</h2>
      </div>
      <ReactECharts option={option} style={{ height: 360 }} />
      <table>
        <thead>
          <tr>
            <th>名称</th>
            {showProviderChannel && <th>渠道</th>}
            <th>token</th>
            <th>占比</th>
            <th>费用</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.name}>
              <td>{row.name}</td>
              {showProviderChannel && <td>{providerChannel(row.name)}</td>}
              <td>{formatTokens(row.total_tokens)}</td>
              <td>{(row.share * 100).toFixed(1)}%</td>
              <td>
                {formatCost(row.cost, row.unpriced)}
                {row.unpriced ? " · 单价未配置" : ""}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

function Sessions({
  rows,
  turns,
  selected,
  onSelect,
}: {
  rows: SessionRow[];
  turns: TurnRow[];
  selected: { id: string; source: string } | null;
  onSelect: (session: { id: string; source: string }) => void;
}) {
  return (
    <>
      <h2>Top 会话</h2>
      <table>
        <thead>
          <tr>
            <th>会话</th>
            <th>来源</th>
            <th>项目</th>
            <th>token</th>
            <th>起止</th>
            <th>原始文件</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={`${row.source}-${row.session_id}`}
              className="clickable"
              onClick={() => onSelect({ id: row.session_id, source: row.source })}
            >
              <td>{row.session_id}</td>
              <td>{row.source}</td>
              <td>{row.project}</td>
              <td>{formatTokens(row.total_tokens)}</td>
              <td>
                {row.started_at} → {row.ended_at}
              </td>
              <td>{row.source_file}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {selected && (
        <>
          <h3>
            会话 {selected.id}（{selected.source}）每轮明细
          </h3>
          <table>
            <thead>
              <tr>
                <th>时间</th>
                <th>模型</th>
                <th>输入</th>
                <th>输出</th>
                <th>缓存读</th>
                <th>缓存写</th>
                <th>推理</th>
                <th>总量</th>
                <th>费用</th>
                <th>原始文件</th>
              </tr>
            </thead>
            <tbody>
              {turns.map((turn, index) => (
                <tr key={`${turn.occurred_at}-${index}`}>
                  <td>{turn.occurred_at}</td>
                  <td>{turn.model}</td>
                  <td>{formatTokens(turn.input_tokens)}</td>
                  <td>{formatTokens(turn.output_tokens)}</td>
                  <td>{formatTokens(turn.cache_read_tokens)}</td>
                  <td>{formatTokens(turn.cache_creation_tokens)}</td>
                  <td>{formatTokens(turn.reasoning_tokens)}</td>
                  <td>{formatTokens(turn.total_tokens)}</td>
                  <td>
                    {formatCost(turn.cost, turn.unpriced)}
                    {turn.cost_note ? ` · ${turn.cost_note}` : ""}
                  </td>
                  <td>{turn.source_file}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </>
  );
}

function CursorPanel({ summary }: { summary: CodeVolumeSummary | null }) {
  if (!summary) {
    return <div className="card partition">暂无 Cursor 代码量数据</div>;
  }
  return (
    <div className="card partition">
      <h2>Cursor 代码量（独立口径，不计入 token）</h2>
      <p>此面板只展示 AI 生成代码行数与占比，不会并入上方任何 token 总量。</p>
      <div className="cards">
        <div className="card">
          <div className="label">提交数</div>
          <div className="value">{formatTokens(summary.commit_count)}</div>
        </div>
        <div className="card">
          <div className="label">新增行</div>
          <div className="value">{formatTokens(summary.lines_added)}</div>
        </div>
        <div className="card">
          <div className="label">AI 生成行</div>
          <div className="value">{formatTokens(summary.composer_lines_added)}</div>
        </div>
        <div className="card">
          <div className="label">AI 占比</div>
          <div className="value">
            {summary.ai_percentage == null ? "—" : `${summary.ai_percentage.toFixed(1)}%`}
          </div>
        </div>
      </div>
    </div>
  );
}

function Settings({
  prices,
  onChange,
  onSave,
}: {
  prices: PriceTable;
  onChange: (prices: PriceTable) => void;
  onSave: () => void;
}) {
  function update(index: number, patch: Partial<PriceEntry>) {
    const next = prices.prices.map((row, i) => (i === index ? { ...row, ...patch } : row));
    onChange({ prices: next });
  }
  return (
    <div className="card">
      <div className="section-title">
        <h2>单价配置</h2>
        <div>
          <button
            className="ghost-btn"
            onClick={() =>
              onChange({
                prices: [
                  ...prices.prices,
                  {
                    model: "",
                    provider: null,
                    input: 0,
                    output: 0,
                    cache_read: 0,
                    cache_creation: 0,
                  },
                ],
              })
            }
          >
            新增
          </button>{" "}
          <button className="ghost-btn" onClick={onSave}>
            保存
          </button>
        </div>
      </div>
      {prices.prices.map((row, index) => (
        <div className="price-row" key={index}>
          <input
            placeholder="模型"
            value={row.model}
            onChange={(e) => update(index, { model: e.target.value })}
          />
          <input
            placeholder="provider（可空）"
            value={row.provider ?? ""}
            onChange={(e) => update(index, { provider: e.target.value || null })}
          />
          <input
            type="number"
            placeholder="输入"
            value={row.input}
            onChange={(e) => update(index, { input: Number(e.target.value) })}
          />
          <input
            type="number"
            placeholder="输出"
            value={row.output}
            onChange={(e) => update(index, { output: Number(e.target.value) })}
          />
          <input
            type="number"
            placeholder="缓存读"
            value={row.cache_read}
            onChange={(e) => update(index, { cache_read: Number(e.target.value) })}
          />
          <input
            type="number"
            placeholder="缓存写"
            value={row.cache_creation}
            onChange={(e) => update(index, { cache_creation: Number(e.target.value) })}
          />
          <button
            className="ghost-btn"
            onClick={() =>
              onChange({ prices: prices.prices.filter((_, i) => i !== index) })
            }
          >
            删除
          </button>
        </div>
      ))}
    </div>
  );
}
