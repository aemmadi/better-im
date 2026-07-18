// Phase 4 — Media / attachment gallery.
// Contract: `api.listMedia(chatId, limit, offset)` -> MediaItemDto[] (newest
// first). Files are rendered through the Tauri asset protocol via
// `convertFileSrc(absolutePath)`. Kind filtering is client-side because the
// endpoint takes no kind argument (see the report note).

import { useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useMediaFeed } from "../queries";
import { useContactMap } from "../lib/contacts";
import { useInView } from "../lib/useInView";
import { formatDay, formatFull } from "../lib/format";
import type { MediaItemDto } from "../types";

type Kind = "all" | "image" | "video" | "audio" | "file";

const CHIPS: { id: Kind; label: string }[] = [
  { id: "all", label: "All" },
  { id: "image", label: "Images" },
  { id: "video", label: "Videos" },
  { id: "audio", label: "Audio" },
  { id: "file", label: "Files" },
];

const KIND_ICON: Record<string, string> = {
  image: "🖼",
  video: "🎬",
  audio: "🎵",
  file: "📄",
};

function itemKey(item: MediaItemDto, i: number): string {
  return `${item.messageId}:${item.absolutePath ?? i}`;
}

export function GalleryView({ chatId }: { chatId: number | null }) {
  const feed = useMediaFeed(chatId);
  const [kind, setKind] = useState<Kind>("all");
  const [lightbox, setLightbox] = useState<MediaItemDto | null>(null);

  const items = useMemo(
    () => feed.data?.pages.flat() ?? [],
    [feed.data],
  );

  const visible = useMemo(
    () => (kind === "all" ? items : items.filter((m) => m.kind === kind)),
    [items, kind],
  );

  const senderHandles = useMemo(
    () => items.filter((m) => !m.isFromMe).map((m) => m.sender),
    [items],
  );
  const contacts = useContactMap(senderHandles);
  const senderName = (m: MediaItemDto) =>
    m.isFromMe ? "You" : contacts.name(m.sender);

  const sentinelRef = useInView(() => {
    if (feed.hasNextPage && !feed.isFetchingNextPage) feed.fetchNextPage();
  }, feed.hasNextPage ?? false);

  const scopeLabel = chatId === null ? "all conversations" : "this conversation";

  return (
    <div className="feature-view gallery">
      <div className="feature-header">
        <span className="feature-title">Media</span>
        <div className="chip-row">
          {CHIPS.map((c) => (
            <button
              key={c.id}
              type="button"
              className={`chip${kind === c.id ? " active" : ""}`}
              onClick={() => setKind(c.id)}
            >
              {c.label}
            </button>
          ))}
        </div>
      </div>

      <div className="feature-scroll">
        {feed.isLoading ? (
          <div className="feature-state muted">Loading media…</div>
        ) : feed.isError ? (
          <div className="feature-state">
            Could not load media.{" "}
            <button className="link-button" onClick={() => feed.refetch()}>
              Retry
            </button>
          </div>
        ) : visible.length === 0 ? (
          <div className="feature-state muted">
            {items.length === 0
              ? `No media shared in ${scopeLabel} yet.`
              : `No ${kind} items here.`}
          </div>
        ) : (
          <>
            <div className="media-grid">
              {visible.map((m, i) => (
                <MediaCell
                  key={itemKey(m, i)}
                  item={m}
                  caption={`${senderName(m)} · ${formatDay(m.timestamp)}`}
                  onOpen={() => setLightbox(m)}
                />
              ))}
            </div>
            <div ref={sentinelRef} className="feed-sentinel">
              {feed.isFetchingNextPage ? (
                <span className="muted">Loading more…</span>
              ) : feed.hasNextPage ? (
                <button className="link-button" onClick={() => feed.fetchNextPage()}>
                  Load more
                </button>
              ) : (
                <span className="muted">End of media</span>
              )}
            </div>
          </>
        )}
      </div>

      {lightbox && (
        <Lightbox
          item={lightbox}
          caption={`${senderName(lightbox)} · ${formatFull(lightbox.timestamp)}`}
          onClose={() => setLightbox(null)}
        />
      )}
    </div>
  );
}

function MediaCell({
  item,
  caption,
  onOpen,
}: {
  item: MediaItemDto;
  caption: string;
  onOpen: () => void;
}) {
  const [broken, setBroken] = useState(false);
  const src = item.absolutePath ? convertFileSrc(item.absolutePath) : null;
  const previewable =
    !!src && !broken && (item.kind === "image" || item.kind === "video");

  return (
    <button
      type="button"
      className="media-cell"
      onClick={previewable ? onOpen : undefined}
      title={item.filename ?? undefined}
    >
      {previewable && item.kind === "image" ? (
        <img
          className="media-thumb"
          src={src!}
          loading="lazy"
          alt={item.filename ?? "Image"}
          onError={() => setBroken(true)}
        />
      ) : previewable && item.kind === "video" ? (
        <>
          <video
            className="media-thumb"
            src={src!}
            preload="metadata"
            muted
            onError={() => setBroken(true)}
          />
          <span className="media-play" aria-hidden>
            ▶
          </span>
        </>
      ) : (
        <span className="media-placeholder">
          <span className="media-placeholder-icon" aria-hidden>
            {KIND_ICON[item.kind] ?? "📄"}
          </span>
          <span className="media-placeholder-name">
            {item.filename ?? item.kind}
          </span>
        </span>
      )}
      <span className="media-caption">{caption}</span>
    </button>
  );
}

function Lightbox({
  item,
  caption,
  onClose,
}: {
  item: MediaItemDto;
  caption: string;
  onClose: () => void;
}) {
  const src = item.absolutePath ? convertFileSrc(item.absolutePath) : null;
  return (
    <div className="lightbox" role="dialog" aria-modal onClick={onClose}>
      <button type="button" className="lightbox-close" onClick={onClose}>
        ✕
      </button>
      <div className="lightbox-body" onClick={(e) => e.stopPropagation()}>
        {src && item.kind === "video" ? (
          <video className="lightbox-media" src={src} controls autoPlay />
        ) : src ? (
          <img className="lightbox-media" src={src} alt={item.filename ?? "Image"} />
        ) : (
          <div className="feature-state muted">This file is unavailable.</div>
        )}
        <div className="lightbox-caption">{caption}</div>
      </div>
    </div>
  );
}
