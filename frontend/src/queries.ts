// React Query hooks over the Tauri command layer, plus the `index-updated`
// live-update subscription.

import {
  useQuery,
  useQueryClient,
  keepPreviousData,
} from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { api } from "./api";
import type { SyncReportDto } from "./types";

/** How many messages to load per thread page. */
export const THREAD_PAGE = 150;

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

/**
 * Subscribe to backend `index-updated` events and invalidate the views that
 * depend on live data (conversation list, the open thread, and index status).
 */
export function useIndexUpdates(openChatId: number | null) {
  const qc = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<SyncReportDto>("index-updated", () => {
      qc.invalidateQueries({ queryKey: ["conversations"] });
      qc.invalidateQueries({ queryKey: ["indexStatus"] });
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
