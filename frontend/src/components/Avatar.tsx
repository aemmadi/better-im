import { avatarColor, resolveAvatarInitial } from "../lib/contacts";

interface Props {
  /** Name used for the initial + deterministic color when there's no photo. */
  name: string;
  /** Contact photo as a data URL, if available. */
  url?: string | null;
  /** `sm` renders the compact avatar used inline in message bubbles. */
  size?: "md" | "sm";
}

/** Circular avatar: the contact's photo when present, else a colored initials
 * circle. Shared by the sidebar and message bubbles. */
export function Avatar({ name, url, size = "md" }: Props) {
  const cls = `avatar${size === "sm" ? " avatar-sm" : ""}`;
  if (url) {
    return (
      <span
        className={`${cls} avatar-img`}
        style={{ backgroundImage: `url(${url})` }}
        aria-hidden
      />
    );
  }
  return (
    <span className={cls} style={{ background: avatarColor(name) }} aria-hidden>
      {resolveAvatarInitial(name)}
    </span>
  );
}
