// Shared TypeScript types mirroring the Rust DTOs in `src-tauri/src/dto.rs`.
// Keep these in lockstep with that file — it is the single source of truth.

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
