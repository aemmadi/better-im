// Phase 4 — Links & shared-content hub. Filled in by the frontend feature agent.
// Contract: `api.listLinks(chatId, limit, offset)` -> LinkItemDto[].

export function LinksView({ chatId }: { chatId: number | null }) {
  return (
    <div className="feature-view feature-placeholder">
      Links hub{chatId === null ? " · all conversations" : ""} — coming in Phase 4.
    </div>
  );
}
