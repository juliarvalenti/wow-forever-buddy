import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "./styles/d.css";
import "./styles/characters.css";
import "./styles/settings.css";

function render() {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

// `npm run build:mock` sets VITE_MOCK: a fake backend for screenshots
// (src/dev/mock-ipc.ts). In every other build this branch is constant-false,
// so the mock module isn't bundled at all.
if (import.meta.env.VITE_MOCK) {
  import("./dev/mock-ipc").then((m) => {
    m.installMockIpc();
    render();
  });
} else {
  render();
}
