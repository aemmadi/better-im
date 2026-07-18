// Typed wrappers over the Tauri `invoke` bridge. Command names and argument
// shapes match `src-tauri/src/commands.rs` (Tauri converts camelCase JS keys to
// snake_case Rust params automatically).

import { invoke } from "@tauri-apps/api/core";
import type {
  ConversationDto,
  FdaStatus,
  IndexStatusDto,
  MessageDto,
  SearchResultDto,
  SyncReportDto,
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
};
