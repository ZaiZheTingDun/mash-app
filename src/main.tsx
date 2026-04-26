import React from "react";
import ReactDOM from "react-dom/client";
import { Theme } from "@radix-ui/themes";
import App from "./App";
import { DebugCanvasWindow } from "./components/DebugCanvasWindow";
import "./App.css";

// Tauri spawns secondary windows pointing at the same SPA bundle and
// distinguishes them by URL hash. `#debug-canvas` swaps the entire
// app for the standalone popout view; everything else renders the main
// app shell. Keep this list short — additional popouts should add a
// new hash and a new top-level component, not new app-level state.
const isDebugCanvas = window.location.hash === "#debug-canvas";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Theme appearance="light" accentColor="blue" radius="medium">
      {isDebugCanvas ? <DebugCanvasWindow /> : <App />}
    </Theme>
  </React.StrictMode>,
);
