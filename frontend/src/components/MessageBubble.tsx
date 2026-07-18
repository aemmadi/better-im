import { forwardRef } from "react";
import type { MessageDto } from "../types";
import { formatTime, formatFull } from "../lib/format";
import { Avatar } from "./Avatar";

interface Props {
  message: MessageDto;
  /** Show the sender name/avatar above the bubble (group threads / first of a run). */
  showSender: boolean;
  /** Resolved sender display name (Contacts name or formatted handle). */
  senderName: string;
  /** Resolved sender avatar, if the contact has a photo. */
  senderAvatarUrl?: string | null;
  highlighted?: boolean;
}

function attachmentLabel(m: MessageDto): string {
  if (m.hasPhoto) return "Photo / Video";
  if (m.hasAttachment || m.numAttachments > 0) return "Attachment";
  return "";
}

/** A single chat bubble. `forwardRef` so the virtualizer can measure height. */
export const MessageBubble = forwardRef<HTMLDivElement, Props>(
  ({ message, showSender, senderName, senderAvatarUrl, highlighted }, ref) => {
    const mine = message.isFromMe;
    const attachment = attachmentLabel(message);
    const hasText = !!message.text && message.text.length > 0;

    return (
      <div
        ref={ref}
        className={`bubble-row ${mine ? "mine" : "theirs"}${highlighted ? " highlighted" : ""}`}
      >
        <div className="bubble-column">
          {showSender && !mine && (
            <span className="bubble-sender-row">
              <Avatar name={senderName} url={senderAvatarUrl} size="sm" />
              <span className="bubble-sender">{senderName}</span>
            </span>
          )}
          <div className="bubble">
            {attachment && (
              <div className="attachment-placeholder">
                <span className="attachment-icon" aria-hidden>
                  {message.hasPhoto ? "🖼" : "📎"}
                </span>
                <span>{attachment}</span>
                {message.numAttachments > 1 && (
                  <span className="attachment-count">×{message.numAttachments}</span>
                )}
              </div>
            )}
            {hasText && <span className="bubble-text">{message.text}</span>}
            {!hasText && !attachment && (
              <span className="bubble-text muted">Message has no text.</span>
            )}
          </div>
          <span className="bubble-meta" title={formatFull(message.timestamp)}>
            {formatTime(message.timestamp)}
            {message.isEdited && <span className="edited"> · Edited</span>}
          </span>
        </div>
      </div>
    );
  },
);

MessageBubble.displayName = "MessageBubble";
