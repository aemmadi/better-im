import { forwardRef } from "react";
import type { MessageDto } from "../types";
import { formatTime, formatFull } from "../lib/format";
import { resolveSenderName } from "../lib/contacts";

interface Props {
  message: MessageDto;
  /** Show the sender name above the bubble (group threads / first of a run). */
  showSender: boolean;
  highlighted?: boolean;
}

function attachmentLabel(m: MessageDto): string {
  if (m.hasPhoto) return "Photo / Video";
  if (m.hasAttachment || m.numAttachments > 0) return "Attachment";
  return "";
}

/** A single chat bubble. `forwardRef` so the virtualizer can measure height. */
export const MessageBubble = forwardRef<HTMLDivElement, Props>(
  ({ message, showSender, highlighted }, ref) => {
    const mine = message.isFromMe;
    const sender = resolveSenderName(mine, message.sender);
    const attachment = attachmentLabel(message);
    const hasText = !!message.text && message.text.length > 0;

    return (
      <div
        ref={ref}
        className={`bubble-row ${mine ? "mine" : "theirs"}${highlighted ? " highlighted" : ""}`}
      >
        <div className="bubble-column">
          {showSender && !mine && <span className="bubble-sender">{sender}</span>}
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
