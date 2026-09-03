import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
