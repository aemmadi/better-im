import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { MessageBubble } from "./MessageBubble";
import { useMessageContext, useThread, THREAD_PAGE } from "../queries";
import { api } from "../api";
import type { MessageDto } from "../types";

interface Props {
  chatId: number;
  title: string;
  /** When set, load context around this message and highlight it. */
  focusMessageId: number | null;
}

function shouldShowSender(messages: MessageDto[], index: number): boolean {
  const m = messages[index];
  if (m.isFromMe) return false;
  const prev = messages[index - 1];
  if (!prev) return true;
  return prev.isFromMe || prev.sender !== m.sender;
}

export function ThreadView({ chatId, title, focusMessageId }: Props) {
  const isContext = focusMessageId != null;
  const threadQuery = useThread(isContext ? null : chatId);
  const contextQuery = useMessageContext(focusMessageId);
  const activeQuery = isContext ? contextQuery : threadQuery;

  const base = useMemo(() => activeQuery.data ?? [], [activeQuery.data]);

  // Older pages loaded via the `before` cursor (thread mode only), prepended.
  const [olderPages, setOlderPages] = useState<MessageDto[]>([]);
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const [reachedStart, setReachedStart] = useState(false);

  // Reset pagination when the target thread / focus changes.
  useEffect(() => {
    setOlderPages([]);
    setLoadingEarlier(false);
    setReachedStart(false);
  }, [chatId, focusMessageId]);

  const messages = useMemo(
    () => (isContext ? base : [...olderPages, ...base]),
    [isContext, base, olderPages],
  );

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 72,
    overscan: 12,
  });

  // Scroll behavior: to the focused message in context mode, else to the bottom
  // on first load of a thread. `didScrollKey` guards against re-running on the
  // background refetches triggered by `index-updated`.
  const didScrollKey = useRef<string | null>(null);
  useEffect(() => {
    if (messages.length === 0) return;
    const key = `${chatId}:${focusMessageId ?? "-"}`;
    if (didScrollKey.current === key) return;
    didScrollKey.current = key;

    requestAnimationFrame(() => {
      if (isContext) {
        const idx = messages.findIndex((m) => m.id === focusMessageId);
        virtualizer.scrollToIndex(idx >= 0 ? idx : 0, { align: "center" });
      } else {
        virtualizer.scrollToIndex(messages.length - 1, { align: "end" });
      }
    });
  }, [messages, chatId, focusMessageId, isContext, virtualizer]);

  const loadEarlier = useCallback(async () => {
    if (isContext || loadingEarlier || reachedStart) return;
    const oldest = messages[0];
    if (!oldest?.timestamp) return;
    setLoadingEarlier(true);
    try {
      const page = await api.getThread(chatId, THREAD_PAGE, oldest.timestamp);
      const fresh = page.filter((m) => m.id !== oldest.id);
      if (fresh.length === 0) setReachedStart(true);
      else setOlderPages((prev) => [...fresh, ...prev]);
    } catch {
      // Surface nothing intrusive; the button stays available to retry.
    } finally {
      setLoadingEarlier(false);
    }
  }, [chatId, isContext, loadingEarlier, reachedStart, messages]);

  if (activeQuery.isLoading) {
    return <div className="thread-empty">Loading messages…</div>;
  }
  if (activeQuery.isError) {
    return (
      <div className="thread-empty">
        Could not load this conversation.
        <button className="link-button" onClick={() => activeQuery.refetch()}>
          Retry
        </button>
      </div>
    );
  }
  if (messages.length === 0) {
    return <div className="thread-empty">No messages in this conversation yet.</div>;
  }

  return (
    <div className="thread">
      <div className="thread-header">
        <span className="thread-title">{title}</span>
        {isContext && <span className="thread-badge">Search context</span>}
      </div>
      <div ref={parentRef} className="thread-scroll">
        {!isContext && (
          <div className="load-earlier">
            {reachedStart ? (
              <span className="muted">Beginning of conversation</span>
            ) : (
              <button
                className="link-button"
                onClick={loadEarlier}
                disabled={loadingEarlier}
              >
                {loadingEarlier ? "Loading…" : "Load earlier messages"}
              </button>
            )}
          </div>
        )}
        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative", width: "100%" }}
        >
          {virtualizer.getVirtualItems().map((row) => {
            const m = messages[row.index];
            return (
              <div
                key={m.id}
                data-index={row.index}
                ref={virtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${row.start}px)`,
                }}
              >
                <MessageBubble
                  message={m}
                  showSender={shouldShowSender(messages, row.index)}
                  highlighted={isContext && m.id === focusMessageId}
                />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
