// Phase 4 — Insights & stats.
// Contract: `api.getInsights(chatId)` -> InsightsDto. Charts via `recharts`.

import { useMemo } from "react";
import {
  ResponsiveContainer,
  AreaChart,
  Area,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  PieChart,
  Pie,
  Cell,
  Legend,
} from "recharts";
import { useInsights } from "../queries";
import { useContactMap } from "../lib/contacts";
import { formatFull } from "../lib/format";
import type { InsightsDto } from "../types";

const C_SENT = "#0a84ff";
const C_RECEIVED = "#30d158";
const C_HOUR = "#0a84ff";
const C_CONTACT = "#5e5ce6";

const TOOLTIP_STYLE: React.CSSProperties = {
  background: "var(--bg-elevated)",
  border: "1px solid var(--border)",
  borderRadius: 8,
  color: "var(--text)",
  fontSize: 12,
  boxShadow: "0 6px 20px var(--shadow)",
};
const TOOLTIP_LABEL: React.CSSProperties = { color: "var(--text-secondary)" };

const dayTickFmt = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});

function fmtDayTick(date: string): string {
  const d = new Date(date);
  return Number.isNaN(d.getTime()) ? date : dayTickFmt.format(d);
}

function fmtHourTick(hour: number): string {
  if (hour === 0) return "12a";
  if (hour === 12) return "12p";
  return hour < 12 ? `${hour}a` : `${hour - 12}p`;
}

export function InsightsView({ chatId }: { chatId: number | null }) {
  const { data, isLoading, isError, refetch } = useInsights(chatId);

  const contactHandles = useMemo(
    () => (data?.topContacts ?? []).map((c) => c.handle),
    [data],
  );
  const contacts = useContactMap(contactHandles);

  const byHour = useMemo(() => {
    const map = new Map<number, number>();
    for (const h of data?.byHour ?? []) map.set(h.hour, h.count);
    return Array.from({ length: 24 }, (_, hour) => ({
      hour,
      count: map.get(hour) ?? 0,
    }));
  }, [data]);

  const topContacts = useMemo(
    () =>
      (data?.topContacts ?? []).slice(0, 8).map((c) => ({
        name: contacts.name(c.handle),
        count: c.count,
      })),
    [data, contacts],
  );

  if (isLoading) {
    return (
      <div className="feature-view insights">
        <div className="feature-state muted">Loading insights…</div>
      </div>
    );
  }
  if (isError || !data) {
    return (
      <div className="feature-view insights">
        <div className="feature-state">
          Could not load insights.{" "}
          <button className="link-button" onClick={() => refetch()}>
            Retry
          </button>
        </div>
      </div>
    );
  }

  const empty = data.totalMessages === 0;
  const scopeLabel = chatId === null ? "all conversations" : "this conversation";

  return (
    <div className="feature-view insights">
      <div className="feature-header">
        <span className="feature-title">Insights</span>
        <span className="muted feature-subtle">{scopeLabel}</span>
      </div>

      <div className="feature-scroll">
        {empty ? (
          <div className="feature-state muted">
            No messages to analyze in {scopeLabel} yet.
          </div>
        ) : (
          <div className="insights-body">
            <KpiRow data={data} />

            <section className="chart-box">
              <h3 className="chart-title">Messages over time</h3>
              <div className="chart-canvas">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart
                    data={data.byDay}
                    margin={{ top: 8, right: 12, bottom: 0, left: -8 }}
                  >
                    <defs>
                      <linearGradient id="ig-day" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor={C_SENT} stopOpacity={0.35} />
                        <stop offset="100%" stopColor={C_SENT} stopOpacity={0} />
                      </linearGradient>
                    </defs>
                    <CartesianGrid stroke="currentColor" opacity={0.12} vertical={false} />
                    <XAxis
                      dataKey="date"
                      tickFormatter={fmtDayTick}
                      tick={{ fill: "currentColor", fontSize: 11 }}
                      axisLine={{ stroke: "currentColor", opacity: 0.15 }}
                      tickLine={false}
                      minTickGap={40}
                    />
                    <YAxis
                      allowDecimals={false}
                      width={36}
                      tick={{ fill: "currentColor", fontSize: 11 }}
                      axisLine={false}
                      tickLine={false}
                    />
                    <Tooltip
                      contentStyle={TOOLTIP_STYLE}
                      labelStyle={TOOLTIP_LABEL}
                      labelFormatter={(v) => fmtDayTick(String(v))}
                    />
                    <Area
                      type="monotone"
                      dataKey="count"
                      name="Messages"
                      stroke={C_SENT}
                      strokeWidth={2}
                      fill="url(#ig-day)"
                    />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </section>

            <div className="chart-grid">
              <section className="chart-box">
                <h3 className="chart-title">Activity by hour</h3>
                <div className="chart-canvas">
                  <ResponsiveContainer width="100%" height="100%">
                    <BarChart
                      data={byHour}
                      margin={{ top: 8, right: 8, bottom: 0, left: -12 }}
                    >
                      <CartesianGrid stroke="currentColor" opacity={0.12} vertical={false} />
                      <XAxis
                        dataKey="hour"
                        tickFormatter={fmtHourTick}
                        interval={2}
                        tick={{ fill: "currentColor", fontSize: 11 }}
                        axisLine={{ stroke: "currentColor", opacity: 0.15 }}
                        tickLine={false}
                      />
                      <YAxis
                        allowDecimals={false}
                        width={36}
                        tick={{ fill: "currentColor", fontSize: 11 }}
                        axisLine={false}
                        tickLine={false}
                      />
                      <Tooltip
                        cursor={{ fill: "currentColor", opacity: 0.06 }}
                        contentStyle={TOOLTIP_STYLE}
                        labelStyle={TOOLTIP_LABEL}
                        labelFormatter={(v) => fmtHourTick(Number(v))}
                      />
                      <Bar dataKey="count" name="Messages" fill={C_HOUR} radius={[3, 3, 0, 0]} />
                    </BarChart>
                  </ResponsiveContainer>
                </div>
              </section>

              <section className="chart-box">
                <h3 className="chart-title">Sent vs received</h3>
                <div className="chart-canvas">
                  <ResponsiveContainer width="100%" height="100%">
                    <PieChart>
                      <Pie
                        data={[
                          { name: "Sent", value: data.sentCount },
                          { name: "Received", value: data.receivedCount },
                        ]}
                        dataKey="value"
                        nameKey="name"
                        innerRadius="55%"
                        outerRadius="80%"
                        paddingAngle={2}
                        stroke="none"
                      >
                        <Cell fill={C_SENT} />
                        <Cell fill={C_RECEIVED} />
                      </Pie>
                      <Legend
                        verticalAlign="bottom"
                        height={24}
                        formatter={(value) => (
                          <span style={{ color: "var(--text-secondary)", fontSize: 12 }}>
                            {value}
                          </span>
                        )}
                      />
                      <Tooltip contentStyle={TOOLTIP_STYLE} labelStyle={TOOLTIP_LABEL} />
                    </PieChart>
                  </ResponsiveContainer>
                </div>
              </section>
            </div>

            {topContacts.length > 0 && (
              <section className="chart-box">
                <h3 className="chart-title">Top contacts</h3>
                <div
                  className="chart-canvas"
                  style={{ height: Math.max(160, topContacts.length * 34 + 24) }}
                >
                  <ResponsiveContainer width="100%" height="100%">
                    <BarChart
                      data={topContacts}
                      layout="vertical"
                      margin={{ top: 4, right: 16, bottom: 4, left: 8 }}
                    >
                      <CartesianGrid stroke="currentColor" opacity={0.12} horizontal={false} />
                      <XAxis
                        type="number"
                        allowDecimals={false}
                        tick={{ fill: "currentColor", fontSize: 11 }}
                        axisLine={false}
                        tickLine={false}
                      />
                      <YAxis
                        type="category"
                        dataKey="name"
                        width={120}
                        tick={{ fill: "currentColor", fontSize: 11 }}
                        axisLine={false}
                        tickLine={false}
                      />
                      <Tooltip
                        cursor={{ fill: "currentColor", opacity: 0.06 }}
                        contentStyle={TOOLTIP_STYLE}
                        labelStyle={TOOLTIP_LABEL}
                      />
                      <Bar dataKey="count" name="Messages" fill={C_CONTACT} radius={[0, 3, 3, 0]} />
                    </BarChart>
                  </ResponsiveContainer>
                </div>
              </section>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function KpiRow({ data }: { data: InsightsDto }) {
  const pct =
    data.totalMessages > 0
      ? Math.round((data.sentCount / data.totalMessages) * 100)
      : 0;
  const tiles: { label: string; value: string; sub?: string }[] = [
    { label: "Total messages", value: data.totalMessages.toLocaleString() },
    { label: "Sent", value: data.sentCount.toLocaleString(), sub: `${pct}%` },
    {
      label: "Received",
      value: data.receivedCount.toLocaleString(),
      sub: `${100 - pct}%`,
    },
    { label: "First message", value: formatFull(data.firstMessage) || "—" },
    { label: "Last message", value: formatFull(data.lastMessage) || "—" },
  ];
  return (
    <div className="kpi-row">
      {tiles.map((t) => (
        <div key={t.label} className="kpi-tile">
          <span className="kpi-label">{t.label}</span>
          <span className="kpi-value">{t.value}</span>
          {t.sub && <span className="kpi-sub muted">{t.sub}</span>}
        </div>
      ))}
    </div>
  );
}
