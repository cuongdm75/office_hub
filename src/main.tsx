import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./ErrorBoundary";
import "./index.css";

// Catch any unhandled errors and show them on screen to prevent silent white screens
function renderError(title: string, msg: string, detail: string) {
  const div = document.createElement("div");
  div.style.cssText = "padding: 20px; color: red; background: white; font-family: monospace;";
  const h2 = document.createElement("h2");
  h2.textContent = title;
  const p1 = document.createElement("p");
  p1.textContent = msg;
  const pre = document.createElement("pre");
  pre.textContent = detail;
  div.appendChild(h2);
  div.appendChild(p1);
  div.appendChild(pre);
  document.body.innerHTML = "";
  document.body.appendChild(div);
}

window.onerror = function(msg, url, lineNo, columnNo, error) {
  renderError(
    "Global Error Caught",
    `Message: ${msg}\nSource: ${url}:${lineNo}:${columnNo}`,
    error?.stack || ''
  );
  return false;
};

window.addEventListener("unhandledrejection", function(event) {
  renderError(
    "Unhandled Promise Rejection",
    `Reason: ${event.reason?.message || event.reason}`,
    event.reason?.stack || ''
  );
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>
);
