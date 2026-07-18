// Shared TypeScript types mirroring the Rust DTOs in `src-tauri/src/dto.rs` and
// `src-tauri/src/features/*.rs`. Keep these in lockstep with those files — they
// are the single source of truth.

export interface FdaStatus {
  granted: boolean;
}

export interface ConversationDto {
  id: number;
  identifier: string;
  displayName: string | null;
  /** Best display label today; Phase 3 swaps in a Contacts-resolved name. */
  label: string;
  service: string | null;
  participants: string[];
}

export interface AttachmentDto {
  id: number;
  filename: string | null;
  mimeType: string | null;
  totalBytes: number;
  isSticker: boolean;
}

export interface MessageDto {
  id: number;
  guid: string;
  chatId: number | null;
  sender: string | null;
  isFromMe: boolean;
  service: string | null;
  text: string | null;
  /** ISO-8601 string, or null when the timestamp is unknown. */
  timestamp: string | null;
  numAttachments: number;
  attachments: AttachmentDto[];
  isEdited: boolean;
  isReply: boolean;
  hasAttachment: boolean;
  hasPhoto: boolean;
  itemType: number;
}

export interface SearchResultDto {
  id: number;
  chatId: number | null;
  canonicalChatId: number | null;
  timestamp: string | null;
  sender: string | null;
  isFromMe: boolean;
  chatName: string | null;
  chatIdentifier: string | null;
  /** FTS snippet with matched spans wrapped in `[` … `]`. */
  snippet: string;
  score: number;
}

export interface SyncReportDto {
  indexed: number;
  watermark: number;
}

export interface IndexStatusDto {
  count: number;
  lastSynced: string | null;
}

/** Resolved identity for one `chat.db` handle. Keyed by the requested identifier
 * in the `resolve_contacts` response. Unmatched handles still carry a formatted
 * `displayName` (with `matched: false`). */
export interface ContactInfoDto {
  displayName: string;
  avatarDataUrl: string | null;
  matched: boolean;
}

// ── Phase 4 feature DTOs (frozen contract) ─────────────────────────────────

export interface MediaItemDto {
  messageId: number;
  chatId: number | null;
  filename: string | null;
  mimeType: string | null;
  /** Absolute path under ~/Library/Messages/Attachments (asset-protocol source). */
  absolutePath: string | null;
  /** "image" | "video" | "audio" | "file". */
  kind: string;
  timestamp: string | null;
  sender: string | null;
  isFromMe: boolean;
}

export interface LinkItemDto {
  messageId: number;
  chatId: number | null;
  url: string;
  timestamp: string | null;
  sender: string | null;
  isFromMe: boolean;
  chatName: string | null;
}

export interface DayCountDto {
  date: string;
  count: number;
}

export interface HourCountDto {
  hour: number;
  count: number;
}

export interface ContactCountDto {
  handle: string;
  count: number;
}

export interface InsightsDto {
  totalMessages: number;
  sentCount: number;
  receivedCount: number;
  firstMessage: string | null;
  lastMessage: string | null;
  byDay: DayCountDto[];
  byHour: HourCountDto[];
  topContacts: ContactCountDto[];
}

export interface TimelineItemDto {
  id: number;
  chatId: number | null;
  chatLabel: string | null;
  sender: string | null;
  isFromMe: boolean;
  text: string | null;
  timestamp: string | null;
  hasAttachment: boolean;
  hasPhoto: boolean;
}
