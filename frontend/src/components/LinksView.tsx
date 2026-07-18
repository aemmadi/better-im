// Phase 4 — Links & shared-content hub.
// Contract: `api.listLinks(chatId, limit, offset)` -> LinkItemDto[] (newest
// first). Links open via the native `open_url` command, falling back to
// clipboard copy when that command isn't available / fails.

import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLinksFeed } from "../queries";
import {
  useContactMap,
  avatarColor,
  resolveAvatarInitial,
} from "../lib/contacts";
import { useInView } from "../lib/useInView";
import { formatFull } from "../lib/format";
import type { LinkItemDto } from "../types";

/** Host portion of a URL, `www.` stripped; best-effort for unparseable input. */
function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url.replace(/^[a-z]+:\/\//i, "").split(/[/?#]/)[0] || url;
  }
}

export function LinksView({ chatId }: { chatId: number | null }) {
  const feed = useLinksFeed(chatId);
  const [copiedId, setCopiedId] = useState<number | null>(null);

  const items = useMemo(() => feed.data?.pages.flat() ?? [], [feed.data]);

  const senderHandles = useMemo(
    () => items.filter((l) => !l.isFromMe).map((l) => l.sender),
    [items],
  );
  const contacts = useContactMap(senderHandles);
  const senderName = (l: LinkItemDto) =>
    l.isFromMe ? "You" : contacts.name(l.sender);

  const sentinelRef = useInView(() => {
    if (feed.hasNextPage && !feed.isFetchingNextPage) feed.fetchNextPage();
  }, feed.hasNextPage ?? false);

  const copy = async (url: string, id: number) => {
    try {
      await navigator.clipboard.writeText(url);
      setCopiedId(id);
      window.setTimeout(
        () => setCopiedId((c) => (c === id ? null : c)),
        1500,
      );
    } catch {
      /* clipboard blocked — nothing further we can do */
    }
  };

  const open = async (url: string, id: number) => {
    try {
      await invoke("open_url", { url });
    } catch {
      // Command missing or rejected — degrade to copying the URL.
      await copy(url, id);
    }
  };

  const scopeLabel = chatId === null ? "all conversations" : "this conversation";

  return (
    <div className="feature-view links">
      <div className="feature-header">
        <span className="feature-title">Links</span>
        {items.length > 0 && (
          <span className="muted feature-subtle">
            {items.length.toLocaleString()} loaded
          </span>
        )}
      </div>

      <div className="feature-scroll">
        {feed.isLoading ? (
          <div className="feature-state muted">Loading links…</div>
        ) : feed.isError ? (
          <div className="feature-state">
            Could not load links.{" "}
            <button className="link-button" onClick={() => feed.refetch()}>
              Retry
            </button>
          </div>
        ) : items.length === 0 ? (
          <div className="feature-state muted">
            No links shared in {scopeLabel} yet.
          </div>
        ) : (
          <>
            <ul className="link-list">
              {items.map((l, i) => {
                const domain = domainOf(l.url);
                return (
                  <li key={`${l.messageId}:${i}`} className="link-row">
                    <span
                      className="link-favicon"
                      style={{ background: avatarColor(domain) }}
                      aria-hidden
                    >
                      {resolveAvatarInitial(domain)}
                    </span>
                    <button
                      type="button"
                      className="link-main"
                      onClick={() => open(l.url, l.messageId)}
                      title={l.url}
                    >
                      <span className="link-domain">{domain}</span>
                      <span className="link-url">{l.url}</span>
                      <span className="link-meta">
                        <span className="link-sender">{senderName(l)}</span>
                        {l.chatName && (
                          <>
                            <span className="dot">·</span>
                            <span>{l.chatName}</span>
                          </>
                        )}
                        <span className="dot">·</span>
                        <span>{formatFull(l.timestamp)}</span>
                      </span>
                    </button>
                    <button
                      type="button"
                      className="link-copy"
                      onClick={() => copy(l.url, l.messageId)}
                    >
                      {copiedId === l.messageId ? "Copied" : "Copy"}
                    </button>
                  </li>
                );
              })}
            </ul>
            <div ref={sentinelRef} className="feed-sentinel">
              {feed.isFetchingNextPage ? (
                <span className="muted">Loading more…</span>
              ) : feed.hasNextPage ? (
                <button className="link-button" onClick={() => feed.fetchNextPage()}>
                  Load more
                </button>
              ) : (
                <span className="muted">End of links</span>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
