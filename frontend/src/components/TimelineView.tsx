// Phase 4 — Global unified timeline.
// Contract: `api.timelineFeed(before, limit)` -> TimelineItemDto[] (newest-first,
// keyset-paginated on the ISO `before` cursor). Virtualized with react-virtual;
// pages are flattened, de-duplicated by id (guards against overlap at a cursor
// boundary where rows share a timestamp), and appended as the user scrolls down.

import { useEffect, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useTimelineFeed } from "../queries";
import { useContactMap } from "../lib/contacts";
import { formatRelative } from "../lib/relativeTime";
import { formatFull } from "../lib/format";
import type { TimelineItemDto } from "../types";

function preview(item: TimelineItemDto): string {
  if (item.text && item.text.trim().length > 0) return item.text;
  if (item.hasPhoto) return "Photo / Video";
  if (item.hasAttachment) return "Attachment";
  return "No text";
}

export function TimelineView() {
  const feed = useTimelineFeed();

  const items = useMemo(() => {
    const seen = new Set<number>();
    const out: TimelineItemDto[] = [];
    for (const row of feed.data?.pages.flat() ?? []) {
      if (seen.has(row.id)) continue;
      seen.add(row.id);
      out.push(row);
    }
    return out;
  }, [feed.data]);

  const senderHandles = useMemo(
    () => items.filter((i) => !i.isFromMe).map((i) => i.sender),
    [items],
  );
  const contacts = useContactMap(senderHandles);
  const senderName = (i: TimelineItemDto) =>
    i.isFromMe ? "You" : contacts.name(i.sender);

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 66,
    overscan: 10,
  });

  const virtualItems = virtualizer.getVirtualItems();
  const lastIndex = virtualItems.length
    ? virtualItems[virtualItems.length - 1].index
    : 0;

  // Fetch the next (older) page as the tail of the list comes into view.
  useEffect(() => {
    if (
      lastIndex >= items.length - 12 &&
      feed.hasNextPage &&
      !feed.isFetchingNextPage
    ) {
      feed.fetchNextPage();
    }
  }, [lastIndex, items.length, feed.hasNextPage, feed.isFetchingNextPage, feed]);

  return (
    <div className="feature-view timeline">
      <div className="feature-header">
        <span className="feature-title">Timeline</span>
        <span className="muted feature-subtle">all conversations · newest first</span>
      </div>

      {feed.isLoading ? (
        <div className="feature-state muted">Loading timeline…</div>
      ) : feed.isError ? (
        <div className="feature-state">
          Could not load the timeline.{" "}
          <button className="link-button" onClick={() => feed.refetch()}>
            Retry
          </button>
        </div>
      ) : items.length === 0 ? (
        <div className="feature-state muted">Nothing here yet.</div>
      ) : (
        <div ref={parentRef} className="feature-scroll timeline-scroll">
          <div
            style={{
              height: virtualizer.getTotalSize(),
              position: "relative",
              width: "100%",
            }}
          >
            {virtualItems.map((row) => {
              const item = items[row.index];
              return (
                <div
                  key={item.id}
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
                  <div className="timeline-row">
                    <div className="timeline-main">
                      <div className="timeline-top">
                        <span className="timeline-sender">{senderName(item)}</span>
                        {item.chatLabel && (
                          <span className="timeline-chat">{item.chatLabel}</span>
                        )}
                        <span
                          className="timeline-time"
                          title={formatFull(item.timestamp)}
                        >
                          {formatRelative(item.timestamp)}
                        </span>
                      </div>
                      <div className="timeline-preview">
                        {(item.hasPhoto || item.hasAttachment) && (
                          <span className="timeline-attach" aria-hidden>
                            {item.hasPhoto ? "🖼" : "📎"}
                          </span>
                        )}
                        <span className="timeline-text">{preview(item)}</span>
                      </div>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
          <div className="feed-sentinel">
            {feed.isFetchingNextPage ? (
              <span className="muted">Loading more…</span>
            ) : feed.hasNextPage ? (
              <button className="link-button" onClick={() => feed.fetchNextPage()}>
                Load more
              </button>
            ) : (
              <span className="muted">Beginning of history</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
