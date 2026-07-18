// Phase 4 — Media / attachment gallery. Filled in by the frontend feature agent.
// Contract: `api.listMedia(chatId, limit, offset)` -> MediaItemDto[].

export function GalleryView({ chatId }: { chatId: number | null }) {
  return (
    <div className="feature-view feature-placeholder">
      Media gallery{chatId === null ? " · all conversations" : ""} — coming in Phase 4.
    </div>
  );
}
