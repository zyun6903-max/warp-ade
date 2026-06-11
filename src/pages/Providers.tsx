import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../i18n/zh";
import { ProviderForm } from "./ProviderForm";
import type { Provider } from "../types";

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
  const dragIdRef = useRef<string | null>(null);

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
        <button type="button" className="btn-primary" onClick={openNew}>
          + {zh.providers.addBtn}
        </button>
      </header>

      {message && <p className="form-message list-toast">{message}</p>}

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
