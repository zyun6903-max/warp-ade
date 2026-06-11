import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../i18n/zh";
import type { Provider, SaveProviderInput, TestProviderResult } from "../types";

const emptyForm: SaveProviderInput = {
  name: "",
  baseUrl: "https://api.anthropic.com",
  apiFormat: "anthropic_messages",
  models: ["claude-sonnet-4-20250514"],
  defaultModel: "claude-sonnet-4-20250514",
  enabled: true,
};

type ProviderFormProps = {
  provider: Provider | null;
  onBack: () => void;
  onSaved: () => void;
};

export function ProviderForm({ provider, onBack, onSaved }: ProviderFormProps) {
  const isEdit = provider !== null;
  const [form, setForm] = useState<SaveProviderInput>(() =>
    provider
      ? {
          id: provider.id,
          name: provider.name,
          baseUrl: provider.baseUrl,
          apiFormat: provider.apiFormat,
          models: provider.models,
          defaultModel: provider.defaultModel,
          enabled: provider.enabled,
        }
      : emptyForm,
  );
  const [modelsText, setModelsText] = useState(
    () => provider?.models.join(", ") ?? "claude-sonnet-4-20250514",
  );
  const [apiKey, setApiKey] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);

  async function handleTest() {
    setMessage(null);

    if (!form.id && !apiKey.trim()) {
      setMessage(zh.providers.needApiKeyForTest);
      return;
    }

    const models = modelsText
      .split(",")
      .map((m) => m.trim())
      .filter(Boolean);

    setTesting(true);
    try {
      const result = await invoke<TestProviderResult>("test_provider", {
        input: {
          providerId: form.id,
          baseUrl: form.baseUrl,
          apiFormat: form.apiFormat,
          defaultModel: form.defaultModel || models[0] || "",
          apiKey: apiKey.trim() || undefined,
        },
      });
      setMessage(result.message);
    } catch (err) {
      setMessage(String(err));
    } finally {
      setTesting(false);
    }
  }

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    setMessage(null);

    if (!form.id && !apiKey.trim()) {
      setMessage(zh.providers.needApiKey);
      return;
    }

    const models = modelsText
      .split(",")
      .map((m) => m.trim())
      .filter(Boolean);

    setSaving(true);
    try {
      const saved = await invoke<Provider>("save_provider", {
        input: {
          id: form.id,
          name: form.name,
          baseUrl: form.baseUrl,
          apiFormat: form.apiFormat,
          models,
          defaultModel: form.defaultModel || models[0] || "",
          enabled: form.enabled,
          apiKey: apiKey.trim() || undefined,
        },
      });
      if (!saved.hasKey) {
        setMessage(`${zh.providers.savedOk}（${zh.providers.noKey}）`);
        return;
      }
      onSaved();
    } catch (err) {
      setMessage(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="page provider-form-page">
      <div className="provider-form-top">
        <button type="button" className="form-back-btn" onClick={onBack}>
          ← {zh.providers.back}
        </button>

        <header className="page-header">
          <h2>{isEdit ? zh.providers.edit : zh.providers.add}</h2>
          <p className="muted">{zh.providers.formSubtitle}</p>
        </header>
      </div>

      <section className="card provider-form-card">
        <form id="provider-form" className="provider-form" onSubmit={handleSave}>
          <div className="provider-form-body">
            <label>
              {zh.providers.name}
              <input
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
              />
            </label>
            <label>
              {zh.providers.baseUrl}
              <input
                value={form.baseUrl}
                onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
                required
              />
            </label>
            <label>
              {zh.providers.apiFormat}
              <select
                value={form.apiFormat}
                onChange={(e) => setForm({ ...form, apiFormat: e.target.value })}
              >
              <option value="anthropic_messages">{zh.providers.anthropic}</option>
              <option value="openai_chat">{zh.providers.openai}</option>
              </select>
            </label>
            <label>
              {zh.providers.models}
              <input value={modelsText} onChange={(e) => setModelsText(e.target.value)} />
            </label>
            <label>
              {zh.providers.defaultModel}
              <input
                value={form.defaultModel}
                onChange={(e) => setForm({ ...form, defaultModel: e.target.value })}
              />
            </label>
            <div className="toggle-row">
              <div className="toggle-copy">
                <span className="toggle-title">{zh.providers.enabled}</span>
                <span className="toggle-desc">{zh.providers.enabledHint}</span>
              </div>
              <label className="toggle-switch">
                <input
                  type="checkbox"
                  checked={form.enabled}
                  onChange={(e) => setForm({ ...form, enabled: e.target.checked })}
                />
                <span className="toggle-track" aria-hidden="true">
                  <span className="toggle-thumb" />
                </span>
              </label>
            </div>
            <label>
              {zh.providers.apiKey}
              {isEdit ? ` ${zh.providers.apiKeyKeep}` : ""}
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={zh.providers.apiKeyPlaceholder}
                required={!isEdit}
              />
            </label>
          </div>
        </form>
        <div className="provider-form-footer">
          {message && <p className="form-message">{message}</p>}
          <div className="form-actions">
            <button type="button" className="btn-ghost" onClick={onBack} disabled={saving || testing}>
              {zh.providers.cancel}
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={handleTest}
              disabled={saving || testing}
            >
              {testing ? zh.providers.testing : zh.providers.testConnection}
            </button>
            <button type="submit" form="provider-form" className="btn-primary" disabled={saving || testing}>
              {saving ? zh.providers.saving : zh.providers.save}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
