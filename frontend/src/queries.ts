// React Query hooks over the Tauri command layer, plus the `index-updated`
// live-update subscription.

import {
  useQuery,
  useInfiniteQuery,
  useQueryClient,
  keepPreviousData,
} from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { api } from "./api";
import type { SyncReportDto } from "./types";

/** How many messages to load per thread page. */
export const THREAD_PAGE = 150;

/** Page sizes for the Phase 4 feature feeds. */
export const MEDIA_PAGE = 60;
export const LINKS_PAGE = 50;
export const TIMELINE_PAGE = 80;

export function useFdaStatus() {
  return useQuery({
    queryKey: ["fda"],
    queryFn: api.fdaStatus,
    staleTime: 5_000,
    retry: false,
  });
}

export function useConversations(enabled: boolean) {
  return useQuery({
    queryKey: ["conversations"],
    queryFn: api.listConversations,
    enabled,
    retry: false,
  });
}

export function useThread(chatId: number | null) {
  return useQuery({
    queryKey: ["thread", chatId],
    queryFn: () => api.getThread(chatId as number, THREAD_PAGE, null),
    enabled: chatId != null,
    retry: false,
  });
}

export function useMessageContext(messageId: number | null) {
  return useQuery({
    queryKey: ["context", messageId],
    queryFn: () => api.getMessageContext(messageId as number, 25, 25),
    enabled: messageId != null,
    retry: false,
  });
}

export function useSearch(query: string) {
  const trimmed = query.trim();
  return useQuery({
    queryKey: ["search", trimmed],
    queryFn: () => api.search(trimmed, 60, 0),
    enabled: trimmed.length > 0,
    placeholderData: keepPreviousData,
    retry: false,
  });
}

export function useIndexStatus(enabled: boolean) {
  return useQuery({
    queryKey: ["indexStatus"],
    queryFn: api.indexStatus,
    enabled,
    retry: false,
  });
}

// ── Phase 4 feature feeds ────────────────────────────────────────────────────

/**
 * Media attachments for a conversation (or all when `chatId` is null), paged by
 * offset. `chatId` participates in the query key so switching scope refetches
 * from a clean first page.
 */
export function useMediaFeed(chatId: number | null) {
  return useInfiniteQuery({
    queryKey: ["media", chatId],
    queryFn: ({ pageParam }) => api.listMedia(chatId, MEDIA_PAGE, pageParam),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) =>
      lastPage.length < MEDIA_PAGE ? undefined : allPages.length * MEDIA_PAGE,
    retry: false,
  });
}

/** Shared links for a conversation (or all), paged by offset. */
export function useLinksFeed(chatId: number | null) {
  return useInfiniteQuery({
    queryKey: ["links", chatId],
    queryFn: ({ pageParam }) => api.listLinks(chatId, LINKS_PAGE, pageParam),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) =>
      lastPage.length < LINKS_PAGE ? undefined : allPages.length * LINKS_PAGE,
    retry: false,
  });
}

/** Aggregate insights for a conversation (or all). */
export function useInsights(chatId: number | null) {
  return useQuery({
    queryKey: ["insights", chatId],
    queryFn: () => api.getInsights(chatId),
    retry: false,
  });
}

/**
 * Global merged timeline, newest-first, keyset-paginated on the last row's ISO
 * `timestamp`. A page shorter than the limit — or one whose final row has no
 * timestamp to seek from — ends pagination.
 */
export function useTimelineFeed() {
  return useInfiniteQuery({
    queryKey: ["timeline"],
    queryFn: ({ pageParam }) => api.timelineFeed(pageParam, TIMELINE_PAGE),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => {
      if (lastPage.length < TIMELINE_PAGE) return undefined;
      return lastPage[lastPage.length - 1]?.timestamp ?? undefined;
    },
    retry: false,
  });
}

/**
 * Subscribe to backend `index-updated` events and invalidate the views that
 * depend on live data (conversation list, the open thread, index status, and
 * the Phase 4 feature feeds).
 */
export function useIndexUpdates(openChatId: number | null) {
  const qc = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<SyncReportDto>("index-updated", () => {
      qc.invalidateQueries({ queryKey: ["conversations"] });
      qc.invalidateQueries({ queryKey: ["indexStatus"] });
      qc.invalidateQueries({ queryKey: ["media"] });
      qc.invalidateQueries({ queryKey: ["links"] });
      qc.invalidateQueries({ queryKey: ["insights"] });
      qc.invalidateQueries({ queryKey: ["timeline"] });
      if (openChatId != null) {
        qc.invalidateQueries({ queryKey: ["thread", openChatId] });
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [qc, openChatId]);
}
