import { useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ConversationDto } from "../types";
import { formatHandle, useContactMap, type ContactMap } from "../lib/contacts";
import { Avatar } from "./Avatar";

interface Props {
  conversations: ConversationDto[];
  selectedId: number | null;
  onSelect: (conversation: ConversationDto) => void;
  loading: boolean;
}

/** The single handle to resolve for a direct (1:1) conversation, or `null` for a
 * group / a chat that already has a custom name (which we keep as-is). */
function directHandle(c: ConversationDto): string | null {
  if (c.displayName && c.displayName.trim().length > 0) return null;
  if (c.participants.length > 1) return null;
  return c.participants[0] ?? c.identifier ?? null;
}

/** Resolved row label + optional avatar for a conversation. */
function rowIdentity(c: ConversationDto, contacts: ContactMap) {
  const handle = directHandle(c);
  if (handle) {
    return { name: contacts.name(handle), avatarUrl: contacts.avatar(handle) };
  }
  const name = c.label && c.label.trim().length > 0 ? c.label : formatHandle(c.identifier);
  return { name, avatarUrl: null as string | null };
}

function subtitle(c: ConversationDto): string {
  if (c.participants.length > 1) {
    return `${c.participants.length} people`;
  }
  return c.service ?? c.identifier;
}

export function Sidebar({ conversations, selectedId, onSelect, loading }: Props) {
  const parentRef = useRef<HTMLDivElement>(null);

  // Batch-resolve every direct conversation's handle in one request.
  const handles = useMemo(
    () => conversations.map(directHandle).filter((h): h is string => !!h),
    [conversations],
  );
  const contacts = useContactMap(handles);

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
              const { name, avatarUrl } = rowIdentity(c, contacts);
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
                  <Avatar name={name} url={avatarUrl} />
                  <span className="conversation-text">
                    <span className="conversation-name">{name}</span>
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
