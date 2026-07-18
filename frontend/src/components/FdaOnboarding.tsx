import { useState } from "react";
import { api } from "../api";

interface Props {
  /** Re-query `fda_status`; resolves the gate when access has been granted. */
  onRecheck: () => void;
  rechecking: boolean;
}

export function FdaOnboarding({ onRecheck, rechecking }: Props) {
  const [opening, setOpening] = useState(false);

  const openSettings = async () => {
    setOpening(true);
    try {
      await api.openFdaSettings();
    } catch {
      /* opening System Settings is best-effort */
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="onboarding">
      <div className="onboarding-card">
        <div className="onboarding-icon" aria-hidden>
          🔒
        </div>
        <h1>Full Disk Access needed</h1>
        <p className="onboarding-lead">
          Better iMessage reads your local Messages database
          (<code>~/Library/Messages/chat.db</code>) to show and search your
          conversations. macOS keeps that file behind Full Disk Access.
        </p>
        <p className="onboarding-privacy">
          Everything stays on this Mac. No messages, contacts, or search data
          ever leave the machine.
        </p>

        <ol className="onboarding-steps">
          <li>
            Click <strong>Open Full Disk Access settings</strong> below.
          </li>
          <li>
            In the list, enable <strong>Better iMessage</strong> (use{" "}
            <strong>+</strong> to add it if it is not listed).
          </li>
          <li>
            Quit and reopen Better iMessage, then click{" "}
            <strong>Re-check access</strong>.
          </li>
        </ol>

        <div className="onboarding-actions">
          <button className="primary-button" onClick={openSettings} disabled={opening}>
            {opening ? "Opening…" : "Open Full Disk Access settings"}
          </button>
          <button className="secondary-button" onClick={onRecheck} disabled={rechecking}>
            {rechecking ? "Checking…" : "Re-check access"}
          </button>
        </div>
      </div>
    </div>
  );
}
