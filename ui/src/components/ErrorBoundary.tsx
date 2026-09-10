import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportCrash } from "../lib/crashReport";

interface ErrorBoundaryProps {
  children: ReactNode;
  fallbackTitle?: string;
  onReset?: () => void;
  className?: string;
  /**
   * Where this boundary sits, in the words a person would use — "the transcript", "this
   * turn", "the HUD tab". It names the crash in the log, so the next report says which
   * part of the app broke instead of only that something did.
   */
  surface?: string;
  /** Draw something other than the standard panel — the root uses this for a full screen. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

/**
 * A render crash stops here instead of taking the app with it.
 *
 * React's rule is unforgiving: an error thrown during render and not caught by a boundary
 * unmounts the **entire tree**. With no boundary above it, one bad row in one turn empties
 * the window — no message, no way back, and (until ADR-0053) no record anywhere of what
 * threw. That is what happened to a fifteen-minute turn: the work was finished and safe in
 * Rust, and the only thing that failed was drawing it.
 *
 * So boundaries are placed where the blast radius should stop: around the whole app, around
 * the transcript, and around each turn. A turn that cannot be drawn becomes one red block
 * in a conversation that still scrolls, and the rest of the answer stays readable.
 *
 * Every catch is reported (`crashReport`), so the log has the stack even when the person
 * simply clicks Retry and carries on.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  public override state: ErrorBoundaryState = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  public override componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    reportCrash("render", error, {
      componentStack: errorInfo.componentStack ?? null,
      surface: this.props.surface ?? this.props.fallbackTitle ?? null,
    });
  }

  private handleReset = (): void => {
    this.setState({ hasError: false, error: null });
    this.props.onReset?.();
  };

  public override render(): ReactNode {
    if (this.state.hasError) {
      const error = this.state.error ?? new Error("An unexpected error occurred.");
      if (this.props.fallback) return this.props.fallback(error, this.handleReset);
      return (
        <div
          className={`studio-dock-error ${this.props.className ?? ""}`}
          role="alert"
          style={{
            padding: "16px",
            margin: "12px",
            display: "flex",
            flexDirection: "column",
            gap: "10px",
          }}
        >
          <div>
            <strong>{this.props.fallbackTitle ?? "Something went wrong"}</strong>
            <p
              style={{
                margin: "4px 0 0 0",
                fontSize: "12px",
                opacity: 0.85,
                wordBreak: "break-word",
              }}
            >
              {error.message || "An unexpected error occurred while rendering this panel."}
            </p>
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <button type="button" className="studio-action-btn" onClick={this.handleReset}>
              Retry
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
