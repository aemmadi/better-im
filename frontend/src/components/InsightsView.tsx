// Phase 4 — Insights & stats. Filled in by the frontend feature agent.
// Contract: `api.getInsights(chatId)` -> InsightsDto. Charts via `recharts`.

export function InsightsView({ chatId }: { chatId: number | null }) {
  return (
    <div className="feature-view feature-placeholder">
      Insights{chatId === null ? " · all conversations" : ""} — coming in Phase 4.
    </div>
  );
}
