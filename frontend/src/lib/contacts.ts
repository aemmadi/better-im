// Contact name/avatar resolution (Phase 3).
//
// This is the single seam every "who is this person" lookup funnels through.
// Names/avatars come from the native `resolve_contacts` command (which matches
// message handles against the macOS Contacts store); when Contacts access is
// denied — or a handle simply isn't in Contacts — we fall back to a formatted
// version of the raw handle so the app never depends on the permission.

import { useMemo } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import type { ContactInfoDto } from "../types";

// react-query cache the resolved batches for a long time: Contacts change rarely
// and the native side memoizes per-handle anyway.
const CONTACTS_STALE_MS = 30 * 60_000; // 30 minutes
const CONTACTS_GC_MS = 60 * 60_000; // 1 hour

/** Pretty-print a raw handle for display when it can't be resolved to a contact:
 * US-style phone formatting when recognizable, otherwise the handle unchanged.
 * Mirrors the backend `format_handle` so denied/unmatched looks the same. */
export function formatHandle(identifier: string | null | undefined): string {
  const h = identifier?.trim();
  if (!h) return "Unknown";
  if (h.includes("@")) return h;
  const digits = h.replace(/\D/g, "");
  if (digits.length === 10) {
    return `(${digits.slice(0, 3)}) ${digits.slice(3, 6)}-${digits.slice(6)}`;
  }
  if (digits.length === 11 && digits.startsWith("1")) {
    return `+1 (${digits.slice(1, 4)}) ${digits.slice(4, 7)}-${digits.slice(7)}`;
  }
  return h;
}

/** Single initial for an avatar placeholder. */
export function resolveAvatarInitial(name: string): string {
  const first = name.trim().charAt(0).toUpperCase();
  return /[A-Z0-9]/.test(first) ? first : "#";
}

/** Deterministic, pleasant background color for an initials avatar, derived from
 * the name so the same person always gets the same color. */
export function avatarColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) {
    hash = (hash * 31 + name.charCodeAt(i)) & 0xffffffff;
  }
  const hue = Math.abs(hash) % 360;
  return `hsl(${hue}deg 58% 48%)`;
}

/** A resolved batch of handles, with convenience accessors. Every accessor is
 * null/undefined-safe and falls back to formatted handles, so callers can use it
 * unconditionally (before data loads, when denied, when a handle is missing). */
export interface ContactMap {
  /** Raw resolved entry for a handle, if present. */
  get(identifier: string | null | undefined): ContactInfoDto | undefined;
  /** Display name: resolved contact name, else formatted handle. */
  name(identifier: string | null | undefined): string;
  /** Avatar data URL for a handle, if the matched contact has a photo. */
  avatar(identifier: string | null | undefined): string | null;
}

function makeContactMap(data: Record<string, ContactInfoDto>): ContactMap {
  return {
    get: (id) => (id ? data[id] : undefined),
    name: (id) => (id && data[id] ? data[id].displayName : formatHandle(id)),
    avatar: (id) => (id ? (data[id]?.avatarDataUrl ?? null) : null),
  };
}

/**
 * Resolve a set of handles to contacts, batched and cached via react-query.
 *
 * The query key is the sorted, de-duplicated handle set, so unrelated screens
 * with overlapping handles share cache entries and re-renders don't refetch.
 * When the batch settles it nudges the permission query to refresh (the first
 * resolve is what triggers the OS prompt, flipping notDetermined → authorized/
 * denied).
 */
export function useContactMap(identifiers: (string | null | undefined)[]): ContactMap {
  const qc = useQueryClient();

  const handles = useMemo(() => {
    const set = new Set<string>();
    for (const id of identifiers) {
      const trimmed = id?.trim();
      if (trimmed) set.add(trimmed);
    }
    return Array.from(set).sort();
  }, [identifiers]);

  const query = useQuery({
    queryKey: ["contacts", handles],
    queryFn: async () => {
      const result = await api.resolveContacts(handles);
      // The first successful resolve may have flipped the permission state.
      qc.invalidateQueries({ queryKey: ["contactsPermission"] });
      return result;
    },
    enabled: handles.length > 0,
    staleTime: CONTACTS_STALE_MS,
    gcTime: CONTACTS_GC_MS,
    retry: false,
  });

  return useMemo(() => makeContactMap(query.data ?? {}), [query.data]);
}

export type PermissionState =
  | "authorized"
  | "denied"
  | "restricted"
  | "notDetermined"
  | "unknown";

/** Live Contacts permission status. Used only for the non-blocking denied hint —
 * the app itself never gates on it. */
export function useContactsPermission() {
  const query = useQuery({
    queryKey: ["contactsPermission"],
    queryFn: api.contactsPermissionStatus,
    staleTime: 60_000,
    retry: false,
  });
  const status = (query.data ?? "unknown") as PermissionState;
  return {
    status,
    /** True only when access was actively refused (denied/restricted) — not for
     * the not-yet-asked state, so we don't nag before the prompt appears. */
    isBlocked: status === "denied" || status === "restricted",
  };
}
