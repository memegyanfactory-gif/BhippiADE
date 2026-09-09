import { Component, type ErrorInfo, type ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
  fallbackTitle?: string;
  onReset?: () => void;
  className?: string;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

/**
 * Standard React Error Boundary component.
 * Prevents any render crash inside a drawer or panel from unmounting the whole Studio (blank screen).
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
    console.error("Uncaught error caught by ErrorBoundary:", error, errorInfo);
  }

  private handleReset = (): void => {
    this.setState({ hasError: false, error: null });
    this.props.onReset?.();
  };

  public override render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div
          className={`studio-dock-error ${this.props.className ?? ""}`}
          role="alert"
          style={{ padding: "16px", margin: "12px", display: "flex", flexDirection: "column", gap: "10px" }}
        >
          <div>
            <strong>{this.props.fallbackTitle ?? "Something went wrong"}</strong>
            <p style={{ margin: "4px 0 0 0", fontSize: "12px", opacity: 0.85, wordBreak: "break-word" }}>
              {this.state.error?.message || "An unexpected error occurred while rendering this panel."}
            </p>
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              type="button"
              className="studio-action-btn"
              onClick={this.handleReset}
            >
              Retry
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
