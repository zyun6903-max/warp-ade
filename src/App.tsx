import { useState } from "react";
import { ChatPage } from "./pages/Chat";
import { ProvidersPage } from "./pages/Providers";
import { ImportPage } from "./pages/Import";
import { SettingsPage } from "./pages/Settings";
import { zh } from "./i18n/zh";
import type { Page } from "./types";
import "./App.css";

const nav: { id: Page; label: string; icon: string }[] = [
  { id: "chat", label: zh.nav.chat, icon: "◈" },
  { id: "providers", label: zh.nav.providers, icon: "⬡" },
  { id: "import", label: zh.nav.import, icon: "↥" },
  { id: "settings", label: zh.nav.settings, icon: "⚙" },
];

function App() {
  const [page, setPage] = useState<Page>("chat");
  const [focusSessionId, setFocusSessionId] = useState<string | null>(null);

  function openSessionInChat(sessionId: string) {
    setFocusSessionId(sessionId);
    setPage("chat");
  }

  return (
    <div className="app-shell">
      <aside className="app-rail">
        <div className="brand-mark" title={zh.appName}>
          <span className="brand-icon">W</span>
        </div>
        <nav className="rail-nav">
          {nav.map((item) => (
            <button
              type="button"
              key={item.id}
              className={`rail-btn ${page === item.id ? "active" : ""}`}
              onClick={() => setPage(item.id)}
              title={item.label}
            >
              <span className="rail-icon">{item.icon}</span>
              <span className="rail-label">{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="rail-footer">
          <span className="version-tag">v0.1</span>
        </div>
      </aside>

      <main className="app-main">
        <div className={page === "chat" ? "page-panel" : "page-panel hidden"}>
          <ChatPage
            isActive={page === "chat"}
            focusSessionId={focusSessionId}
            onFocusSessionHandled={() => setFocusSessionId(null)}
          />
        </div>
        <div className={page === "providers" ? "page-panel" : "page-panel hidden"}>
          <ProvidersPage isActive={page === "providers"} />
        </div>
        <div className={page === "import" ? "page-panel" : "page-panel hidden"}>
          <ImportPage onOpenSession={openSessionInChat} />
        </div>
        <div className={page === "settings" ? "page-panel" : "page-panel hidden"}>
          <SettingsPage />
        </div>
      </main>
    </div>
  );
}

export default App;
