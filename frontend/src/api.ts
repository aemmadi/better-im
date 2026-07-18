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
  SemanticIndexReportDto,
  SemanticStatusDto,
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
  /** Action capabilities the backend advertises (stable string tags, e.g.
   * `"SendText"`). Empty today: the shipping build is a read-only provider. The
   * thread composer gates on this — see `ThreadComposer`. */
  capabilities: () => invoke<string[]>("capabilities"),

  fdaStatus: () => invoke<FdaStatus>("fda_status"),

  openFdaSettings: () => invoke<void>("open_fda_settings"),

  listConversations: () => invoke<ConversationDto[]>("list_conversations"),

  getThread: (chatId: number, limit: number, before?: string | null) =>
    invoke<MessageDto[]>("get_thread", { chatId, limit, before: before ?? null }),

  search: (query: string, limit: number, offset: number) =>
    invoke<SearchResultDto[]>("search", { query, limit, offset }),

  /** Phase 5 hybrid (semantic + keyword) search. Same result shape as `search`;
   * `score` is a fused RRF score. Degrades to keyword ranking until the semantic
   * index is built. */
  smartSearch: (query: string, limit: number, offset: number) =>
    invoke<SearchResultDto[]>("smart_search", { query, limit, offset }),

  /** Semantic-index health: whether embeddings exist and how many remain. */
  semanticStatus: () => invoke<SemanticStatusDto>("semantic_status"),

  /** Build (or top up) the semantic index. Emits `semantic-progress` events. */
  buildSemanticIndex: () =>
    invoke<SemanticIndexReportDto>("build_semantic_index"),

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
