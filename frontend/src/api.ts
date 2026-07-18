// Typed wrappers over the Tauri `invoke` bridge. Command names and argument
// shapes match `src-tauri/src/commands.rs` + `src-tauri/src/features/*.rs`
// (Tauri converts camelCase JS keys to snake_case Rust params automatically).

import { invoke } from "@tauri-apps/api/core";
import type {
  ContactInfoDto,
  ConversationDto,
  FdaStatus,
  IndexStatusDto,
  InsightsDto,
  LinkItemDto,
  MediaItemDto,
  MessageDto,
  SearchResultDto,
  SyncReportDto,
  TimelineItemDto,
} from "./types";

/** Sentinel prefix the backend puts on any failure to read `chat.db`. */
export const FDA_DENIED = "FDA_DENIED";

/** Whether a rejected invoke looks like a Full Disk Access denial. */
export function isFdaError(err: unknown): boolean {
  return String(err ?? "").includes(FDA_DENIED);
}

export const api = {
  fdaStatus: () => invoke<FdaStatus>("fda_status"),

  openFdaSettings: () => invoke<void>("open_fda_settings"),

  listConversations: () => invoke<ConversationDto[]>("list_conversations"),

  getThread: (chatId: number, limit: number, before?: string | null) =>
    invoke<MessageDto[]>("get_thread", { chatId, limit, before: before ?? null }),

  search: (query: string, limit: number, offset: number) =>
    invoke<SearchResultDto[]>("search", { query, limit, offset }),

  getMessageContext: (id: number, before: number, after: number) =>
    invoke<MessageDto[]>("get_message_context", { id, before, after }),

  reindex: () => invoke<SyncReportDto>("reindex"),

  indexStatus: () => invoke<IndexStatusDto>("index_status"),

  /** Batch-resolve raw handles (phones/emails) to Contacts identities. Returns a
   * record keyed by the requested identifier. */
  resolveContacts: (identifiers: string[]) =>
    invoke<Record<string, ContactInfoDto>>("resolve_contacts", { identifiers }),

  /** Current Contacts permission: authorized | denied | restricted | notDetermined. */
  contactsPermissionStatus: () => invoke<string>("contacts_permission_status"),

  openContactsSettings: () => invoke<void>("open_contacts_settings"),

  // ── Phase 4 feature endpoints ────────────────────────────────────────────

  /** Media attachments, newest first. `chatId = null` spans all conversations. */
  listMedia: (chatId: number | null, limit: number, offset: number) =>
    invoke<MediaItemDto[]>("list_media", { chatId, limit, offset }),

  /** Shared links, newest first. `chatId = null` spans all conversations. */
  listLinks: (chatId: number | null, limit: number, offset: number) =>
    invoke<LinkItemDto[]>("list_links", { chatId, limit, offset }),

  /** Aggregate insights for one conversation (`chatId`) or all (`null`). */
  getInsights: (chatId: number | null) =>
    invoke<InsightsDto>("get_insights", { chatId }),

  /** Merged cross-conversation feed, newest-first, keyset-paginated on `before`. */
  timelineFeed: (before: string | null, limit: number) =>
    invoke<TimelineItemDto[]>("timeline_feed", { before, limit }),
};
