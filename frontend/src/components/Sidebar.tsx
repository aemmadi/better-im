import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ConversationDto } from "../types";
import { resolveAvatarInitial, resolveDisplayName } from "../lib/contacts";

interface Props {
  conversations: ConversationDto[];
  selectedId: number | null;
  onSelect: (conversation: ConversationDto) => void;
  loading: boolean;
}

function conversationLabel(c: ConversationDto): string {
  // Phase 3: resolveDisplayName will map the identifier to a Contacts name.
  if (c.label && c.label.trim().length > 0) return c.label;
  return resolveDisplayName(c.identifier);
}

function subtitle(c: ConversationDto): string {
  if (c.participants.length > 1) {
    return `${c.participants.length} people`;
  }
  return c.service ?? c.identifier;
}

export function Sidebar({ conversations, selectedId, onSelect, loading }: Props) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: conversations.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 64,
    overscan: 10,
  });

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">Conversations</span>
        <span className="sidebar-count">
          {loading ? "…" : conversations.length}
        </span>
      </div>

      {loading ? (
        <div className="placeholder">Loading conversations…</div>
      ) : conversations.length === 0 ? (
        <div className="placeholder">No conversations found.</div>
      ) : (
        <div ref={parentRef} className="sidebar-scroll">
          <div
            style={{ height: virtualizer.getTotalSize(), position: "relative", width: "100%" }}
          >
            {virtualizer.getVirtualItems().map((row) => {
              const c = conversations[row.index];
              const label = conversationLabel(c);
              const selected = c.id === selectedId;
              return (
                <button
                  key={c.id}
                  type="button"
                  className={`conversation-row${selected ? " selected" : ""}`}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: row.size,
                    transform: `translateY(${row.start}px)`,
                  }}
                  onClick={() => onSelect(c)}
                >
                  <span className="avatar" aria-hidden>
                    {resolveAvatarInitial(label)}
                  </span>
                  <span className="conversation-text">
                    <span className="conversation-name">{label}</span>
                    <span className="conversation-sub">{subtitle(c)}</span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}
    </aside>
  );
}
