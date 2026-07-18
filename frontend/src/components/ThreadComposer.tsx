import { useCapabilities } from "../queries";

/**
 * The read-only composer strip pinned to the bottom of a thread.
 *
 * This is the *visible half* of the send-layer seam (its backend half is the
 * `capabilities` Tauri command + `ReadOnlyProvider` in `core`). It reads the
 * backend's advertised capabilities and enables real sending only when the
 * provider reports `"SendText"`.
 *
 * Today the backend is a read-only provider (empty capability set), so `canSend`
 * is `false` and this renders a polished, disabled field plus a short note. When
 * a send-capable provider is dropped in on the backend, `canSend` flips to `true`
 * and the input + button enable automatically — the only remaining work is to
 * wire the field's submit handler to a future `send_text` command at the marked
 * spot. Nothing else in the UI needs to change.
 */
export function ThreadComposer() {
  const caps = useCapabilities();

  // ── SEND-LAYER GATE ────────────────────────────────────────────────────────
  // The entire send affordance hangs off this one boolean. Read-only today.
  const canSend = caps.data?.includes("SendText") ?? false;

  return (
    <footer
      className={`composer${canSend ? "" : " composer--readonly"}`}
      aria-label={canSend ? "Message composer" : "Message composer (read-only)"}
    >
      <div className="composer-field">
        <input
          className="composer-input"
          type="text"
          placeholder={canSend ? "iMessage" : "Sending isn’t available yet"}
          disabled={!canSend}
          aria-label="Message"
          // FUTURE SEND PATH: when `canSend`, wire onKeyDown(Enter) / a controlled
          // value + a `send_text` Tauri command here. Intentionally a no-op input
          // in the read-only build.
        />
        <button
          type="button"
          className="composer-send"
          disabled={!canSend}
          aria-label="Send message"
          title={canSend ? "Send" : "Sending is a future addition"}
        >
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
            <path
              d="M12 19V6M12 6l-6 6M12 6l6 6"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </div>
      {!canSend && (
        <p className="composer-note">
          Better iMessage is read-only. Sending is a planned addition — it needs a
          lower-security tier (disabling System Integrity Protection), so it will
          ship later as a separate opt-in.
        </p>
      )}
    </footer>
  );
}
