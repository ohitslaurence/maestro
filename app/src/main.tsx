import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/base.css";
import "./styles/sidebar.css";
import "./styles/terminal.css";
import "./styles/diff-viewer.css";
import "./styles/daemon.css";
import "./styles/tabs.css";
import "./styles/opencode.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
