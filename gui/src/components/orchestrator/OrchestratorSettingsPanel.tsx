import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentRole,
  AuthorizedCustomGate,
  LoopIterationLimits,
  OrchestratorConfig,
  OrchestratorProfile,
  OrchestratorQuickSlot,
  ProfileCapability,
  ValidationGateConfig,
} from "../../types/orchestrator";
import { DEFAULT_ITERATION_LIMITS, DEFAULT_ORCHESTRATOR_PROFILES, DEFAULT_ORCHESTRATOR_QUICK_SLOTS, DEFAULT_VALIDATION_GATES } from "../../config/orchestratorPresets";

interface Props {
  t: (key: any) => string;
  onChanged?: () => Promise<void> | void;
}

const CAPABILITIES: ProfileCapability[] = ["reasoning", "review", "workspace_read", "workspace_write", "command_execution"];
type LoadedOrchestratorConfig = OrchestratorConfig & {
  profiles: OrchestratorProfile[];
  quickSlots: OrchestratorQuickSlot[];
  validationGates: ValidationGateConfig[];
  iterationLimits: LoopIterationLimits;
  authorizedCustomGates: AuthorizedCustomGate[];
};

type OllamaModelListResponse =
  | { status: "success"; models: string[] }
  | { status: "error"; code: "connection_failed" | "timeout" | "invalid_endpoint" | "api_error" | "invalid_response" };

function newProfile(): OrchestratorProfile {
  const id = `custom-${crypto.randomUUID()}`;
  return {
    id,
    displayName: "New profile",
    adapter: "provider",
    providerId: "deepseek",
    model: "deepseek-v4.1-flash",
    thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
  };
}

export default function OrchestratorSettingsPanel({ t, onChanged }: Props) {
  const [config, setConfig] = useState<LoadedOrchestratorConfig | null>(null);
  const configRef = useRef<LoadedOrchestratorConfig | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState("");
  const selectedProfileIdRef = useRef("");
  const ollamaRequestGenerationRef = useRef(0);
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaRefreshing, setOllamaRefreshing] = useState(false);
  const [ollamaCustomModelEditing, setOllamaCustomModelEditing] = useState(false);
  const [ollamaModelStatus, setOllamaModelStatus] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    let alive = true;
    invoke<OrchestratorConfig>("get_orchestrator_config")
      .then((loaded) => {
        if (!alive) return;
        const next: LoadedOrchestratorConfig = {
          ...loaded,
          profiles: loaded.profiles ?? DEFAULT_ORCHESTRATOR_PROFILES,
          quickSlots: loaded.quickSlots ?? DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
          validationGates: loaded.validationGates ?? DEFAULT_VALIDATION_GATES,
          iterationLimits: loaded.iterationLimits ?? DEFAULT_ITERATION_LIMITS,
          authorizedCustomGates: loaded.authorizedCustomGates ?? [],
        };
        configRef.current = next;
        setConfig(next);
        selectedProfileIdRef.current = next.profiles[0]?.id ?? "";
        setSelectedProfileId(next.profiles[0]?.id ?? "");
      })
      .catch((reason) => { if (alive) setError(String(reason)); });
    return () => { alive = false; };
  }, []);

  const selectedProfile = useMemo(
    () => config?.profiles.find((profile) => profile.id === selectedProfileId),
    [config, selectedProfileId],
  );

  async function savePatch(
    patch: Partial<OrchestratorConfig>,
    persist: () => Promise<unknown> = () => invoke("update_orchestrator_config", { config: patch }),
  ) {
    const current = configRef.current;
    if (!current) return;
    const next = { ...current, ...patch };
    configRef.current = next;
    setConfig(next);
    setSaving(true);
    setError("");
    const queuedSave = saveQueueRef.current.then(async () => {
      await persist();
      await onChanged?.();
    });
    saveQueueRef.current = queuedSave.catch((reason) => {
      setError(String(reason));
    });
    await saveQueueRef.current;
    setSaving(false);
  }

  if (!config) return <section aria-busy="true">{error || t("orchestrator.settings.loading")}</section>;

  const updateProfile = (patch: Partial<OrchestratorProfile>) => {
    const current = configRef.current;
    if (!selectedProfile || !current) return;
    void savePatch({ profiles: current.profiles.map((profile) => profile.id === selectedProfile.id ? { ...profile, ...patch } : profile) });
  };

  const updateLimits = (patch: Partial<LoopIterationLimits>) => {
    const current = configRef.current;
    if (current) void savePatch({ iterationLimits: { ...current.iterationLimits, ...patch } });
  };

  const updateGate = (gate: ValidationGateConfig, enabled: boolean) => {
    const current = configRef.current;
    if (current) {
      void savePatch(
        { validationGates: current.validationGates.map((item) => item.id === gate.id ? { ...item, enabled } : item) },
        () => invoke("update_validation_gate_enabled", { gateId: gate.id, enabled }),
      );
    }
  };

  const getGateLabel = (gate: ValidationGateConfig) => {
    const knownGateLabelKeys: Record<string, string> = {
      typecheck: "orchestrator.settings.gateTypeCheck",
      test: "orchestrator.settings.gateTests",
      "git-status": "orchestrator.settings.gateRepositoryState",
    };
    const labelKey = knownGateLabelKeys[gate.id];
    return labelKey ? t(labelKey) : gate.name;
  };

  const updateSlot = (slot: OrchestratorQuickSlot, patch: Partial<OrchestratorQuickSlot>) => {
    const current = configRef.current;
    if (current) void savePatch({ quickSlots: current.quickSlots!.map((item) => item.id === slot.id ? { ...item, ...patch } : item) });
  };

  const refreshOllamaModels = async () => {
    if (!selectedProfile || selectedProfile.adapter !== "ollama") return;
    const profileId = selectedProfile.id;
    const endpoint = selectedProfile.ollamaEndpoint?.trim() || undefined;
    const requestGeneration = ++ollamaRequestGenerationRef.current;
    setOllamaRefreshing(true);
    setOllamaModelStatus("");
    try {
      const response = await invoke<OllamaModelListResponse>("list_ollama_models_for_settings", {
        endpoint,
      });
      const currentProfile = configRef.current?.profiles.find((profile) => profile.id === profileId);
      const currentEndpoint = currentProfile?.ollamaEndpoint?.trim() || undefined;
      if (requestGeneration !== ollamaRequestGenerationRef.current ||
          selectedProfileIdRef.current !== profileId || currentEndpoint !== endpoint) return;
      if (response.status === "success") {
        setOllamaModels(response.models);
        setOllamaModelStatus(response.models.length > 0
          ? `${response.models.length} ${t("apiKeyPanel.ollamaLocal.modelsFound")}`
          : t("apiKeyPanel.ollamaLocal.noModelsFound"));
      } else {
        setOllamaModelStatus(t(`apiKeyPanel.ollamaLocal.error.${response.code}`));
      }
    } catch {
      if (requestGeneration === ollamaRequestGenerationRef.current) {
        setOllamaModelStatus(t("apiKeyPanel.ollamaLocal.error.invalid_response"));
      }
    } finally {
      if (requestGeneration === ollamaRequestGenerationRef.current) setOllamaRefreshing(false);
    }
  };

  const selectProfile = (profileId: string) => {
    selectedProfileIdRef.current = profileId;
    ollamaRequestGenerationRef.current += 1;
    setSelectedProfileId(profileId);
    setOllamaRefreshing(false);
    setOllamaCustomModelEditing(false);
    setOllamaModels([]);
    setOllamaModelStatus("");
  };

  return (
    <div className="orchestrator-settings-panel">
      <h2>{t("orchestrator.settings.title")}</h2>
      {error && <p role="alert">{error}</p>}
      <p aria-live="polite">{saving ? t("orchestrator.settings.saving") : ""}</p>

      <section className="orchestrator-settings-section">
        <h3>{t("orchestrator.settings.profiles")}</h3>
        <div className="orchestrator-settings-row">
          <select aria-label={t("orchestrator.settings.selectProfile")} value={selectedProfileId} onChange={(event) => selectProfile(event.target.value)}>
            {config.profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.displayName}</option>)}
          </select>
          <button type="button" disabled={saving} onClick={() => {
            const profile = newProfile();
            void savePatch({ profiles: [...config.profiles, profile] }).then(() => selectProfile(profile.id));
          }}>{t("orchestrator.settings.addProfile")}</button>
          {selectedProfile && <button type="button" disabled={saving} onClick={() => {
            const remaining = config.profiles.filter((profile) => profile.id !== selectedProfile.id);
            void savePatch({ profiles: remaining }).then(() => selectProfile(remaining[0]?.id ?? ""));
          }}>{t("orchestrator.settings.deleteProfile")}</button>}
        </div>
        {selectedProfile && (
          <div className="orchestrator-profile-editor">
            <label>{t("orchestrator.settings.displayName")}<input value={selectedProfile.displayName} onChange={(event) => updateProfile({ displayName: event.target.value })} /></label>
            <label>{t("orchestrator.settings.adapter")}<select value={selectedProfile.adapter} onChange={(event) => updateProfile({ adapter: event.target.value as OrchestratorProfile["adapter"] })}>
              <option value="provider">Provider</option><option value="ollama">Ollama</option><option value="cli">CLI</option>
            </select></label>
            {selectedProfile.adapter === "provider" && <>
              <label>{t("orchestrator.settings.providerId")}<input value={selectedProfile.providerId ?? ""} onChange={(event) => updateProfile({ providerId: event.target.value })} /></label>
              <label>{t("orchestrator.settings.model")}<input value={selectedProfile.model ?? ""} onChange={(event) => updateProfile({ model: event.target.value })} /></label>
            <label>{t("orchestrator.settings.thinkingMode")}<select value={selectedProfile.thinkingMode ?? "thinking"} onChange={(event) => updateProfile({ thinkingMode: event.target.value as OrchestratorProfile["thinkingMode"] })}><option value="thinking">Thinking</option><option value="normal">Normal</option></select></label>
              <label>{t("orchestrator.settings.reasoningEffort")}<input value={selectedProfile.reasoningEffort ?? ""} onChange={(event) => updateProfile({ reasoningEffort: event.target.value || undefined })} /></label>
            </>}
            {selectedProfile.adapter === "ollama" && <>
              <label>{t("orchestrator.settings.endpoint")}<input value={selectedProfile.ollamaEndpoint ?? "http://127.0.0.1:11434"} onChange={(event) => { ollamaRequestGenerationRef.current += 1; setOllamaRefreshing(false); updateProfile({ ollamaEndpoint: event.target.value }); setOllamaModels([]); setOllamaModelStatus(""); }} /></label>
              <label>{t("orchestrator.settings.model")}{ollamaCustomModelEditing ? <><input aria-label={t("apiKeyPanel.ollamaLocal.customModel")} value={selectedProfile.ollamaModel ?? ""} onChange={(event) => updateProfile({ ollamaModel: event.target.value })} /><button type="button" aria-label={t("apiKeyPanel.ollamaLocal.selectInstalledModel")} onClick={() => setOllamaCustomModelEditing(false)}>☷</button></> : <select aria-label={t("apiKeyPanel.ollamaLocal.modelTag")} value={selectedProfile.ollamaModel ?? ""} onChange={(event) => {
                if (event.target.value === "__custom_model__") setOllamaCustomModelEditing(true);
                else updateProfile({ ollamaModel: event.target.value });
              }}>
                {!selectedProfile.ollamaModel && <option value="">{t("apiKeyPanel.ollamaLocal.selectInstalledModel")}</option>}
                {selectedProfile.ollamaModel && !ollamaModels.includes(selectedProfile.ollamaModel) && <option value={selectedProfile.ollamaModel}>{selectedProfile.ollamaModel}</option>}
                {ollamaModels.map((model) => <option key={model} value={model}>{model}</option>)}
                <option value="__custom_model__">{t("apiKeyPanel.ollamaLocal.customModel")}</option>
              </select>}</label>
              <button type="button" disabled={ollamaRefreshing} aria-label={t("apiKeyPanel.ollamaLocal.refresh")} onClick={() => void refreshOllamaModels()}>{ollamaRefreshing ? "…" : "↻"}</button>
              {ollamaModelStatus && <span role="status">{ollamaModelStatus}</span>}
            </>}
            {selectedProfile.adapter === "cli" && <>
              <label>{t("orchestrator.settings.executable")}<input value={selectedProfile.executable ?? "codex"} onChange={(event) => updateProfile({ executable: event.target.value })} /></label>
              <label>{t("orchestrator.settings.arguments")}<input value={(selectedProfile.args ?? []).join(" ")} onChange={(event) => updateProfile({ args: event.target.value.split(/\s+/).filter(Boolean) })} /></label>
            </>}
            <label>{t("orchestrator.settings.contextWindow")}<input type="number" min={1} value={selectedProfile.contextWindowTokens ?? ""} onChange={(event) => updateProfile({ contextWindowTokens: event.target.value ? Number(event.target.value) : undefined })} /></label>
            <fieldset><legend>{t("orchestrator.settings.capabilities")}</legend>
              {CAPABILITIES.map((capability) => <label key={capability}><input type="checkbox" checked={selectedProfile.capabilities.includes(capability)} onChange={(event) => updateProfile({ capabilities: event.target.checked ? [...new Set([...selectedProfile.capabilities, capability])] : selectedProfile.capabilities.filter((value) => value !== capability) })} />{t(`orchestrator.capability.${capability}`)}</label>)}
            </fieldset>
          </div>
        )}
      </section>

      <section className="orchestrator-settings-section">
        <h3>{t("orchestrator.settings.quickSlots")}</h3>
        {config.quickSlots!.map((slot) => <div className="orchestrator-settings-row" key={slot.id}>
          <input aria-label={`${slot.id} ${t("orchestrator.settings.slotLabel")}`} value={slot.label} onChange={(event) => updateSlot(slot, { label: event.target.value })} />
          <select aria-label={`${slot.id} ${t("orchestrator.settings.slotProfile")}`} value={slot.profileId} onChange={(event) => updateSlot(slot, { profileId: event.target.value })}>
            {!config.profiles.some((profile) => profile.id === slot.profileId) && <option value={slot.profileId}>{t("orchestrator.settings.unavailableProfile")} ({slot.profileId})</option>}
            {config.profiles.map((profile) => <option value={profile.id} key={profile.id}>{profile.displayName}</option>)}
          </select>
          <label><input type="checkbox" checked={slot.visible} onChange={(event) => updateSlot(slot, { visible: event.target.checked })} />{t("orchestrator.settings.visible")}</label>
          <input aria-label={`${slot.id} ${t("orchestrator.settings.order")}`} type="number" value={slot.order} onChange={(event) => updateSlot(slot, { order: Number(event.target.value) || 0 })} />
        </div>)}
        <button type="button" onClick={() => void savePatch({ quickSlots: [...config.quickSlots!, { id: `slot-${crypto.randomUUID()}`, profileId: config.profiles[0]?.id ?? "", label: t("orchestrator.settings.newSlot"), visible: true, order: config.quickSlots!.length }] })}>{t("orchestrator.settings.addSlot")}</button>
      </section>

      <section className="orchestrator-settings-section">
        <h3>{t("orchestrator.settings.defaults")}</h3>
        <label>{t("orchestrator.settings.defaultWorkflow")}<select value={config.activeWorkflowId ?? "full_loop"} disabled><option value="full_loop">{t("orchestrator.workflow.fullLoop")}</option></select></label>
        {(["maxPlanReviewIterations", "maxFixIterations", "maxCodeReviewIterations"] as const).map((key) => <label key={key}>{t(`orchestrator.settings.${key}`)}<input type="number" min={1} max={100} value={config.iterationLimits![key]} onChange={(event) => { const value = Number(event.target.value); if (Number.isInteger(value) && value >= 1 && value <= 100) updateLimits({ [key]: value }); }} /></label>)}
      </section>

      <section className="orchestrator-settings-section">
        <h3>{t("orchestrator.settings.validationGates")}</h3>
        <div className="orchestrator-validation-gates">
          {config.validationGates.map((gate) => (
            <div className="orchestrator-validation-gate" key={gate.id}>
              <label>
                <input
                  type="checkbox"
                  checked={gate.enabled}
                  onChange={(event) => updateGate(gate, event.target.checked)}
                />
                {getGateLabel(gate)}
              </label>
              <div className="orchestrator-validation-command">
                <span>{t("orchestrator.settings.configuredCommand")}</span>
                <code>{[gate.executable, ...gate.args].join(" ")}</code>
              </div>
            </div>
          ))}
        </div>
      </section>

      <details className="orchestrator-settings-section orchestrator-settings-diagnostics">
        <summary>{t("orchestrator.settings.advanced")}</summary>
        <p>{t("orchestrator.settings.redactionActive")}</p>
        <p>{t("orchestrator.settings.processIsolationActive")}</p>
        <h4>{t("orchestrator.settings.authorizedCustomGates")}</h4>
        {config.authorizedCustomGates.map((gate: AuthorizedCustomGate) => (
          <code key={`${gate.gateId}-${gate.commandHash}`}>
            {gate.gateId}: {gate.commandHash}
          </code>
        ))}
      </details>
    </div>
  );
}
