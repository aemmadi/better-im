// Contact name/avatar resolution.
//
// PHASE 3 INJECTION POINT: today these are identity functions over the raw
// handle identifier (phone / email) that `core` already provides. Phase 3 will
// replace the bodies here with a real Contacts-framework resolver (name +
// avatar) — every place that renders a person's name funnels through these, so
// swapping them in is the only change needed on the frontend.

/** Human-facing display name for a handle identifier. */
export function resolveDisplayName(identifier: string | null | undefined): string {
  const id = identifier?.trim();
  return id && id.length > 0 ? id : "Unknown";
}

/** A short label for the sender of a message (or "You" for outgoing). */
export function resolveSenderName(
  isFromMe: boolean,
  sender: string | null | undefined,
): string {
  return isFromMe ? "You" : resolveDisplayName(sender);
}

/** Initial used for the avatar placeholder. */
export function resolveAvatarInitial(name: string): string {
  const trimmed = name.trim();
  const first = trimmed.charAt(0).toUpperCase();
  return /[A-Z0-9]/.test(first) ? first : "#";
}
