import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../i18n/zh";
import { DismissibleNotice } from "../components/DismissibleNotice";
import { ProviderForm } from "./ProviderForm";
import type { Provider, ProviderUsageRow } from "../types";

type ProvidersPageProps = {
  isActive: boolean;
};

type Screen = "list" | "form";

export function ProvidersPage({ isActive }: ProvidersPageProps) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [screen, setScreen] = useState<Screen>("list");
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [showUsage, setShowUsage] = useState(false);
  const [usageRows, setUsageRows] = useState<ProviderUsageRow[]>([]);
  const [usageLoading, setUsageLoading] = useState(false);
  const dragIdRef = useRef<string | null>(null);

  async function refreshUsage() {
    setUsageLoading(true);
    try {
      setUsageRows(await invoke<ProviderUsageRow[]>("list_provider_usage"));
    } catch (e) {
      console.error(e);
      setUsageRows([]);
    } finally {
      setUsageLoading(false);
    }
  }

  async function toggleUsage() {
    const next = !showUsage;
    setShowUsage(next);
    if (next) {
      await refreshUsage();
    }
  }

  async function refresh() {
    setProviders(await invoke<Provider[]>("list_providers"));
  }

  useEffect(() => {
    if (!isActive) {
      setScreen("list");
      setEditingProvider(null);
      return;
    }
    refresh().catch(console.error);
  }, [isActive]);

  function openNew() {
    setEditingProvider(null);
    setScreen("form");
    setMessage(null);
  }

  function openEdit(provider: Provider) {
    setEditingProvider(provider);
    setScreen("form");
    setMessage(null);
  }

  function handleBack() {
    setScreen("list");
    setEditingProvider(null);
  }

  async function handleSaved() {
    await refresh();
    setScreen("list");
    setEditingProvider(null);
    setMessage(zh.providers.savedOk);
  }

  async function handleDelete(id: string) {
    await invoke("delete_provider", { providerId: id });
    await refresh();
  }

  async function handleDuplicate(id: string) {
    try {
      await invoke<Provider>("duplicate_provider", { providerId: id });
      await refresh();
      setMessage(zh.providers.copiedOk);
    } catch (err) {
      setMessage(String(err));
    }
  }

  async function persistOrder(next: Provider[]) {
    const ids = next.map((p) => p.id);
    setProviders(next);
    try {
      setProviders(await invoke<Provider[]>("reorder_providers", { ids }));
    } catch (err) {
      setMessage(String(err));
      await refresh();
    }
  }

  function handleDragStart(e: React.DragEvent, id: string) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
    dragIdRef.current = id;
    setDragId(id);
  }

  function handleDragOver(e: React.DragEvent, targetId: string) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const sourceId = dragIdRef.current;
    if (sourceId && sourceId !== targetId) {
      setDragOverId(targetId);
    }
  }

  function handleDragLeave(e: React.DragEvent, targetId: string) {
    if (e.currentTarget.contains(e.relatedTarget as Node)) {
      return;
    }
    setDragOverId((current) => (current === targetId ? null : current));
  }

  function handleDrop(e: React.DragEvent, targetId: string) {
    e.preventDefault();
    e.stopPropagation();

    const sourceId = e.dataTransfer.getData("text/plain") || dragIdRef.current;
    dragIdRef.current = null;
    setDragId(null);
    setDragOverId(null);

    if (!sourceId || sourceId === targetId) {
      return;
    }

    const ids = providers.map((p) => p.id);
    const fromIdx = ids.indexOf(sourceId);
    const toIdx = ids.indexOf(targetId);
    if (fromIdx < 0 || toIdx < 0) {
      return;
    }

    const next = [...providers];
    const [moved] = next.splice(fromIdx, 1);
    next.splice(toIdx, 0, moved);
    void persistOrder(next);
  }

  function handleDragEnd() {
    dragIdRef.current = null;
    setDragId(null);
    setDragOverId(null);
  }

  if (screen === "form") {
    return (
      <ProviderForm provider={editingProvider} onBack={handleBack} onSaved={handleSaved} />
    );
  }

  return (
    <div className="page providers-page">
      <header className="page-header page-header-row">
        <div>
          <h2>{zh.providers.title}</h2>
          <p className="muted">{zh.providers.subtitle}</p>
        </div>
        <div className="page-header-actions">
          <button type="button" className="btn-ghost" onClick={() => void toggleUsage()}>
            {showUsage ? zh.providers.usageHide : zh.providers.usageShow}
          </button>
          <button type="button" className="btn-primary" onClick={openNew}>
            + {zh.providers.addBtn}
          </button>
        </div>
      </header>

      {message && (
        <DismissibleNotice variant="success" className="list-toast" onDismiss={() => setMessage(null)}>
          {message}
        </DismissibleNotice>
      )}

      {showUsage && (
        <section className="card provider-usage-card">
          <div className="provider-usage-head">
            <h3>{zh.providers.usageTitle}</h3>
            <button
              type="button"
              className="btn-ghost btn-sm"
              disabled={usageLoading}
              onClick={() => void refreshUsage()}
            >
              ↻
            </button>
          </div>
          <p className="muted provider-usage-note">{zh.providers.usageEstimateNote}</p>
          {usageLoading ? (
            <p className="muted">{zh.settings.loading}</p>
          ) : usageRows.length === 0 ? (
            <p className="muted">{zh.providers.usageEmpty}</p>
          ) : (
            <>
              <p className="provider-usage-summary muted">
                {zh.providers.usageTotal(
                  usageRows.reduce((n, r) => n + r.requestCount, 0),
                  usageRows.reduce((n, r) => n + r.inputTokens, 0),
                  usageRows.reduce((n, r) => n + r.outputTokens, 0),
                )}
              </p>
              <div className="provider-usage-table-wrap">
                <table className="provider-usage-table">
                  <thead>
                    <tr>
                      <th>{zh.providers.usageProvider}</th>
                      <th>{zh.providers.usageModel}</th>
                      <th>{zh.providers.usageRequests}</th>
                      <th>{zh.providers.usageInput}</th>
                      <th>{zh.providers.usageOutput}</th>
                      <th>{zh.providers.usageTests}</th>
                      <th>{zh.providers.usageLastUsed}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {usageRows.map((row) => (
                      <tr key={`${row.providerId}-${row.model}`}>
                        <td>{row.providerName}</td>
                        <td className="mono" title={row.model}>
                          {row.model}
                        </td>
                        <td>{row.requestCount}</td>
                        <td>{row.inputTokens.toLocaleString()}</td>
                        <td>{row.outputTokens.toLocaleString()}</td>
                        <td>{row.testCount}</td>
                        <td>
                          {row.lastUsedAt
                            ? new Date(row.lastUsedAt * 1000).toLocaleString()
                            : "—"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </section>
      )}

      <div className="provider-list">
        {providers.map((provider) => (
          <div
            key={provider.id}
            className={`provider-item ${dragId === provider.id ? "dragging" : ""} ${dragOverId === provider.id ? "drag-over" : ""}`}
            onDragOver={(e) => handleDragOver(e, provider.id)}
            onDragLeave={(e) => handleDragLeave(e, provider.id)}
            onDrop={(e) => handleDrop(e, provider.id)}
          >
            <div
              className="provider-drag-handle"
              draggable
              role="button"
              tabIndex={0}
              aria-label={zh.providers.dragHint}
              title={zh.providers.dragHint}
              onDragStart={(e) => handleDragStart(e, provider.id)}
              onDragEnd={handleDragEnd}
              onMouseDown={(e) => e.preventDefault()}
            >
              <svg width="10" height="14" viewBox="0 0 10 14" fill="currentColor" aria-hidden="true">
                <circle cx="2" cy="2" r="1.2" />
                <circle cx="8" cy="2" r="1.2" />
                <circle cx="2" cy="7" r="1.2" />
                <circle cx="8" cy="7" r="1.2" />
                <circle cx="2" cy="12" r="1.2" />
                <circle cx="8" cy="12" r="1.2" />
              </svg>
            </div>
            <div className="provider-item-content">
              <span className="provider-name">{provider.name}</span>
              <span className="provider-model">{provider.defaultModel}</span>
              {!provider.hasKey && (
                <span className="provider-tag provider-tag-warn">{zh.providers.noKey}</span>
              )}
              {!provider.enabled && (
                <span className="provider-tag provider-tag-muted">已禁用</span>
              )}
            </div>
            <div className="provider-row-actions">
              <button
                type="button"
                className="provider-action-btn"
                onClick={() => handleDuplicate(provider.id)}
              >
                {zh.providers.copyBtn}
              </button>
              <button
                type="button"
                className="provider-action-btn"
                onClick={() => openEdit(provider)}
              >
                {zh.providers.editBtn}
              </button>
              <button
                type="button"
                className="provider-action-btn provider-action-danger"
                onClick={() => handleDelete(provider.id)}
              >
                {zh.providers.delete}
              </button>
            </div>
          </div>
        ))}
        {providers.length === 0 && (
          <div className="provider-empty">
            <p className="muted">{zh.providers.noProviders}</p>
            <button type="button" className="btn-primary" onClick={openNew}>
              + {zh.providers.addBtn}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
