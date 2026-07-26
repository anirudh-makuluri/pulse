import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Pet from "./Pet";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./styles.css";

const CurrentWindow = getCurrentWebviewWindow().label === "pet" ? Pet : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <CurrentWindow />
  </React.StrictMode>,
);
