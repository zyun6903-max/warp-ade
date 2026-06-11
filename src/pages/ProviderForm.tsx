import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../i18n/zh";
import { DismissibleNotice } from "../components/DismissibleNotice";
import type { Provider, SaveProviderInput, TestProviderResult } from "../types";

const emptyForm: SaveProviderInput = {
  name: "",
  baseUrl: "https://api.anthropic.com",
  apiFormat: "anthropic_messages",
  models: ["claude-sonnet-4-20250514"],
  defaultModel: "claude-sonnet-4-20250514",
  enabled: true,
};

function parseModels(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of text.split(",")) {
    const model = part.trim();
    if (!model || seen.has(model)) continue;
    seen.add(model);
    out.push(model);
  }
  return out;
}

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
  const [messageOk, setMessageOk] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testingModel, setTestingModel] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, TestProviderResult>>({});

  const parsedModels = useMemo(() => parseModels(modelsText), [modelsText]);

  async function runModelTest(model: string) {
    setMessage(null);

    if (!form.id && !apiKey.trim()) {
      setMessageOk(false);
      setMessage(zh.providers.needApiKeyForTest);
      return;
    }

    setTestingModel(model);
    try {
      const result = await invoke<TestProviderResult>("test_provider", {
        input: {
          providerId: form.id,
          baseUrl: form.baseUrl,
          apiFormat: form.apiFormat,
          defaultModel: form.defaultModel || model,
          model,
          apiKey: apiKey.trim() || undefined,
        },
      });
      setTestResults((prev) => ({ ...prev, [model]: result }));
    } catch (err) {
      setTestResults((prev) => ({
        ...prev,
        [model]: {
          ok: false,
          model,
          latencyMs: 0,
          message: String(err),
        },
      }));
    } finally {
      setTestingModel(null);
    }
  }

  async function handleTestAll() {
    for (const model of parsedModels) {
      await runModelTest(model);
    }
  }

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    setMessage(null);

    if (!form.id && !apiKey.trim()) {
      setMessageOk(false);
      setMessage(zh.providers.needApiKey);
      return;
    }

    const models = parseModels(modelsText);
    if (models.length === 0) {
      setMessageOk(false);
      setMessage("请至少填写一个模型");
      return;
    }

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
        setMessageOk(true);
        setMessage(`${zh.providers.savedOk}（${zh.providers.noKey}）`);
        return;
      }
      onSaved();
    } catch (err) {
      setMessageOk(false);
      setMessage(String(err));
    } finally {
      setSaving(false);
    }
  }

  const testing = testingModel !== null;

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

            <div className="provider-models-block">
              <div className="provider-models-head">
                <span className="provider-models-label">{zh.providers.models}</span>
                {parsedModels.length > 1 && (
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    disabled={testing || saving}
                    onClick={() => void handleTestAll()}
                  >
                    {zh.providers.testAll}
                  </button>
                )}
              </div>
              <input
                value={modelsText}
                onChange={(e) => {
                  setModelsText(e.target.value);
                  const models = parseModels(e.target.value);
                  setForm((prev) => ({
                    ...prev,
                    models,
                    defaultModel: models.includes(prev.defaultModel ?? "")
                      ? prev.defaultModel
                      : models[0] ?? "",
                  }));
                }}
                placeholder="model-a, model-b"
              />
              {parsedModels.length > 0 && (
                <ul className="provider-model-list">
                  {parsedModels.map((model) => {
                    const result = testResults[model];
                    const isDefault = form.defaultModel === model;
                    return (
                      <li key={model} className="provider-model-row">
                        <div className="provider-model-main">
                          <span className="provider-model-name" title={model}>
                            {model}
                          </span>
                          {isDefault && (
                            <span className="provider-model-tag">{zh.providers.defaultModelTag}</span>
                          )}
                        </div>
                        <div className="provider-model-actions">
                          {!isDefault && (
                            <button
                              type="button"
                              className="btn-ghost btn-sm"
                              disabled={testing || saving}
                              onClick={() => setForm({ ...form, defaultModel: model })}
                            >
                              {zh.providers.setDefaultModel}
                            </button>
                          )}
                          <button
                            type="button"
                            className="btn-ghost btn-sm"
                            disabled={testing || saving}
                            onClick={() => void runModelTest(model)}
                          >
                            {testingModel === model ? zh.providers.testing : zh.providers.testModel}
                          </button>
                        </div>
                        {result && (
                          <p
                            className={`provider-model-result ${result.ok ? "ok" : "err"}`}
                            title={result.message}
                          >
                            {result.message}
                          </p>
                        )}
                      </li>
                    );
                  })}
                </ul>
              )}
            </div>

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
          {message && (
            <DismissibleNotice
              variant={messageOk ? "success" : "error"}
              onDismiss={() => setMessage(null)}
            >
              {message}
            </DismissibleNotice>
          )}
          <div className="form-actions">
            <button type="button" className="btn-ghost" onClick={onBack} disabled={saving || testing}>
              {zh.providers.cancel}
            </button>
            <button
              type="button"
              className="btn-ghost"
              disabled={saving || testing || parsedModels.length === 0}
              onClick={() => {
                const model = form.defaultModel || parsedModels[0];
                if (model) void runModelTest(model);
              }}
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
