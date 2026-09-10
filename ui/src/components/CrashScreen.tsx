/**
 * What the app shows instead of going blank.
 *
 * The failure this replaces: a render threw, React unmounted everything, and the window
 * became an empty rectangle. Nothing said what happened, nothing said whether the work was
 * lost, and there was no way back except killing the app — which is how a finished
 * fifteen-minute turn stopped being readable.
 *
 * The three things this screen must do, in order of what the person needs:
 *
 * 1. **Say the work is safe.** The conversation lives in Rust, not in the page. Drawing it
 *    failed; producing it did not. Reload brings it back. That sentence is the whole reason
 *    this screen exists, and it is the first thing on it.
 * 2. **Give a way out that is not Task Manager.** Reload re-runs the webview against the
 *    same running backend.
 * 3. **Carry the evidence.** The message, the stack and the component stack, already in the
 *    log, and one button that copies the lot.
 */

import { useState } from "react";

export function CrashScreen({ error, onReload }: { error: Error; onReload: () => void }) {
  const [copied, setCopied] = useState(false);
  const detail = [
    error.message,
    error.stack ?? "",
  ]
    .filter(Boolean)
    .join("\n\n");

  const copy = () => {
    void navigator.clipboard
      ?.writeText(detail)
      .then(() => setCopied(true))
      .catch(() => setCopied(false));
  };

  return (
    <div className="crash-screen" role="alert">
      <div className="crash-card">
        <h1 className="crash-title">The window could not draw this.</h1>
        <p className="crash-lede">
          Your conversation is safe — it lives in Bhippi itself, not in this page, so nothing
          you asked for was lost. Reloading redraws it against the same running app.
        </p>

        <div className="crash-actions">
          <button type="button" className="crash-btn primary" onClick={onReload}>
            Reload the window
          </button>
          <button type="button" className="crash-btn" onClick={copy}>
            {copied ? "Copied" : "Copy the details"}
          </button>
        </div>

        <p className="crash-note">
          The full error is already in the log, at <code>~/.bhippi/logs</code>.
        </p>

        <details className="crash-details">
          <summary>What went wrong</summary>
          <pre className="crash-stack">{detail}</pre>
        </details>
      </div>
    </div>
  );
}
