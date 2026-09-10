import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { CrashScreen } from "./components/CrashScreen";
import { installCrashReporting } from "./lib/crashReport";
// The terminal emulator ships its own layout CSS. It must load before ours so
// styles/cli.css can theme the surface it creates.
import "@xterm/xterm/css/xterm.css";
import "./styles/tokens.css";
import "./styles/motion.css";
import "./styles/app.css";
import "./styles/chat.css";
import "./styles/screens.css";
import "./styles/usage.css";
import "./styles/workbench.css";
import "./styles/activity.css";
import "./styles/phases.css";
import "./styles/fault.css";
import "./styles/cli.css";
import "./styles/multi-workspace.css";
import "./styles/plugins.css";
// Studio overrides load last so the compact command dock wins over the shared chat layout.
import "./styles/studio.css";
import "./styles/hud.css";
import "./styles/splash.css";
import "./styles/crash.css";

// Before anything renders: an exception in an event handler, a timer or a rejected promise
// never reaches a React boundary, and those are the failures that used to leave no trace at
// all (ADR-0053).
installCrashReporting();

/*
 * The root boundary.
 *
 * Without one, React's rule is that an error thrown in render unmounts the entire tree —
 * which is how a finished turn became an empty window with nothing to click and nothing in
 * any log. The app is allowed to break; it is not allowed to disappear.
 */
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary
      surface="the app"
      fallback={(error) => (
        <CrashScreen error={error} onReload={() => window.location.reload()} />
      )}
    >
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
