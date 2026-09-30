import { useEffect, useRef, useState } from "react";
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
import {
  DEFAULT_ITERATION_LIMITS,
  DEFAULT_ORCHESTRATOR_PROFILES,
  DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
  DEFAULT_VALIDATION_GATES,
} from "../../config/orchestratorPresets";
import { BUILTIN_OPENROUTER_MODELS, getOpenRouterModelDisplayName } from "../../config/builtinOpenRouter";
import { getModelDisplayName } from "../../config/modelDisplayNames";
import { getOrchestratorProfileDisplayName } from "../../config/orchestratorProfileDisplayName";
import { getProviderModels, MODEL_CAPABILITIES } from "../../modelCapabilities";
import type { ModelCapabilities, ReasoningEffortOption, ThinkingModePolicy } from "../../modelCapabilities";
import { isReasoningEffortOption, normalizeReasoningEffort } from "../../reasoningEffort";
import {
  ORCHESTRATOR_PROVIDER_GROUPS,
  canonicalizeProfileExecutionFields,
  inferProfileGroupKey,
} from "../../config/orchestratorProviderGroups";

interface Props {
  t: (key: any) => string;
  onChanged?: () => Promise<void> | void;
}

const CAPABILITIES: ProfileCapability[] = ["reasoning", "review", "workspace_read", "workspace_write", "command_execution"];

const ROLE_NAME_KEYS: Record<AgentRole, string> = {
  planner: "orchestrator.roles.planner",
  plan_reviewer: "orchestrator.roles.planReviewer",
  implementer: "orchestrator.roles.implementer",
  fixer: "orchestrator.roles.fixer",
  code_reviewer: "orchestrator.roles.codeReviewer",
};

const EXPANDED_GROUPS_KEY = "anthro-bridge.orchestrator-settings.expanded-providers";

function initialExpandedGroups(): Set<string> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(EXPANDED_GROUPS_KEY) ?? "[]");
    if (!Array.isArray(value)) return new Set();
    const allowed = new Set<string>(ORCHESTRATOR_PROVIDER_GROUPS.map((group) => group.key));
    return new Set(value.filter((key): key is string => typeof key === "string" && allowed.has(key)));
  } catch {
    return new Set();
  }
}

function uniqueProfileId(): string {
  return `custom-${crypto.randomUUID()}`;
}

function defaultProfileForGroup(groupKey: string): OrchestratorProfile {
  const id = uniqueProfileId();
  const base = {
    id,
    displayName: "",
    capabilities: ["reasoning", "review", "workspace_read"] as ProfileCapability[],
  };
  if (groupKey === "ollama") {
    const profile: OrchestratorProfile = { ...base, adapter: "ollama", ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "", contextWindowTokens: 32768 };
    return { ...profile, displayName: getOrchestratorProfileDisplayName(profile) };
  }
  if (groupKey === "cli") {
    const profile: OrchestratorProfile = { ...base, adapter: "cli", executable: "codex", args: ["exec"], capabilities: [...base.capabilities, "workspace_write", "command_execution"], contextWindowTokens: 200000 };
    return { ...profile, displayName: getOrchestratorProfileDisplayName(profile) };
  }
  if (groupKey === "other") {
    const profile: OrchestratorProfile = { ...base, adapter: "provider", providerId: "custom", model: "custom-model", thinkingMode: "thinking", contextWindowTokens: 128000 };
    return { ...profile, displayName: getOrchestratorProfileDisplayName(profile) };
  }
  const group = ORCHESTRATOR_PROVIDER_GROUPS.find((item) => item.key === groupKey);
  const providerId = group?.providerId ?? "deepseek";
  const models = getProviderModels(providerId);
  const preferredModel = providerId === "deepseek" ? "deepseek-v4.1-flash"
    : providerId === "mimo" ? "mimo-v2.6-pro"
      : providerId === "minimax" ? "MiniMax-M3"
        : providerId === "openrouter" ? "openai/gpt-5.6-sol"
          : models[0] ?? "";
  const model = models.includes(preferredModel) || providerId === "deepseek" ? preferredModel : models[0] ?? preferredModel;
  const contextWindowTokens = providerId === "kimi" ? 1_048_576
    : providerId === "kimi-code" ? 200_000
      : providerId === "mimo" || providerId === "minimax" ? 1_000_000
        : providerId === "openrouter" ? 1_050_000
          : 128_000;
  const profile: OrchestratorProfile = {
    ...base,
    adapter: "provider",
    providerId,
    model,
    thinkingMode: "thinking",
    contextWindowTokens,
  };
  const modelPolicy = profileModelPolicy(profile);
  if (modelPolicy.options.length > 0) {
    profile.reasoningEffort = normalizeReasoningEffort("thinking", providerId === "deepseek" ? "high" : "", modelPolicy.options);
  }
  return { ...profile, displayName: getOrchestratorProfileDisplayName(profile) };
}

function profileModelPolicy(profile: OrchestratorProfile): {
  policy: ThinkingModePolicy;
  options: ReasoningEffortOption[];
  forcedEffort?: string;
} {
  if (profile.adapter !== "provider" || !profile.model) return { policy: "unknown", options: [] };
  const capabilities: ModelCapabilities | undefined = profile.providerId === "openrouter"
    ? BUILTIN_OPENROUTER_MODELS[profile.model]?.capabilities ?? MODEL_CAPABILITIES[profile.model]
    : profile.providerId === "deepseek" && profile.model === "deepseek-v4.1-flash"
      ? MODEL_CAPABILITIES["deepseek-flash"]
      : MODEL_CAPABILITIES[profile.model];
  if (!capabilities) return { policy: "toggleable", options: [] };
  const rawOptions = capabilities.reasoningEffortOptions ?? capabilities.forcedThinkingOptions ?? [];
  return {
    policy: capabilities.thinkingModePolicy ?? "toggleable",
    options: rawOptions.filter(isReasoningEffortOption),
    forcedEffort: capabilities.forcedReasoningEffort,
  };
}

function profileGroupSummary(profile: OrchestratorProfile): string {
  return getOrchestratorProfileDisplayName(profile);
}

function assignedRolesForProfile(
  profileId: string,
  assignments?: OrchestratorConfig["assignments"],
): AgentRole[] {
  if (!assignments) return [];
  return Object.entries(assignments).flatMap(([role, assignment]) => {
    const assignedId = typeof assignment === "string" ? assignment : assignment?.profileId;
    return assignedId === profileId ? [role as AgentRole] : [];
  });
}

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

export default function OrchestratorSettingsPanel({ t, onChanged }: Props) {
  const [config, setConfig] = useState<LoadedOrchestratorConfig | null>(null);
  const configRef = useRef<LoadedOrchestratorConfig | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(initialExpandedGroups);
  const [ollamaModels, setOllamaModels] = useState<Record<string, string[]>>({});
  const [ollamaRefreshing, setOllamaRefreshing] = useState<Set<string>>(() => new Set());
  const [ollamaCustomModelEditing, setOllamaCustomModelEditing] = useState<Set<string>>(() => new Set());
  const [ollamaModelStatus, setOllamaModelStatus] = useState<Record<string, string>>({});
  const ollamaRequestGenerationRef = useRef(new Map<string, number>());
  const aliveRef = useRef(true);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    aliveRef.current = true;
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
      })
      .catch((reason) => { if (alive) setError(String(reason)); });
    return () => {
      alive = false;
      aliveRef.current = false;
    };
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(EXPANDED_GROUPS_KEY, JSON.stringify([...expandedGroups]));
    } catch {
      // Expansion state is a convenience; settings remain usable if storage is unavailable.
    }
  }, [expandedGroups]);

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

  const updateProfile = (profileId: string, patch: Partial<OrchestratorProfile>) => {
    const current = configRef.current;
    if (!current) return;
    void savePatch({ profiles: current.profiles.map((profile) => {
      if (profile.id !== profileId) return profile;
      const updated = { ...profile, ...patch };
      const canonical = canonicalizeProfileExecutionFields(updated);
      return { ...canonical, displayName: getOrchestratorProfileDisplayName(canonical) };
    }) });
  };

  const toggleGroup = (groupKey: string, expanded: boolean) => {
    setExpandedGroups((previous) => {
      const next = new Set(previous);
      if (expanded) next.add(groupKey);
      else next.delete(groupKey);
      return next;
    });
  };

  const updateModel = (profile: OrchestratorProfile, model: string) => {
    const nextProfile = { ...profile, model };
    const { policy, options } = profileModelPolicy(nextProfile);
    const patch: Partial<OrchestratorProfile> = { model };
    if (policy === "none") {
      patch.thinkingMode = "normal";
      patch.reasoningEffort = undefined;
    } else if (policy === "thinking_only" || policy === "forced") {
      patch.thinkingMode = "thinking";
      patch.reasoningEffort = options.length
        ? normalizeReasoningEffort("thinking", profile.reasoningEffort ?? "", options)
        : undefined;
    } else if (!options.length || profile.thinkingMode !== "thinking") {
      patch.reasoningEffort = undefined;
    } else {
      patch.reasoningEffort = normalizeReasoningEffort("thinking", profile.reasoningEffort ?? "", options);
    }
    updateProfile(profile.id, patch);
  };

  const updateThinkingMode = (profile: OrchestratorProfile, thinkingMode: "normal" | "thinking") => {
    const { policy, options } = profileModelPolicy(profile);
    const effectiveMode = policy === "thinking_only" || policy === "forced" ? "thinking" : policy === "none" ? "normal" : thinkingMode;
    updateProfile(profile.id, {
      thinkingMode: effectiveMode,
      reasoningEffort: effectiveMode === "thinking" && options.length
        ? normalizeReasoningEffort("thinking", profile.reasoningEffort ?? "", options)
        : undefined,
    });
  };

  const setProfileVisible = (profile: OrchestratorProfile, visible: boolean) => {
    const current = configRef.current;
    if (!current) return;
    const existing = current.quickSlots.find((slot) => slot.profileId === profile.id);
    if (existing) {
      updateSlot(existing, { visible });
      return;
    }
    if (!visible) return;
    const order = current.quickSlots.reduce((max, slot) => Math.max(max, slot.order), -1) + 1;
    void savePatch({ quickSlots: [...current.quickSlots, {
      id: `slot-${crypto.randomUUID()}`,
      profileId: profile.id,
      label: getOrchestratorProfileDisplayName(profile),
      visible: true,
      order,
    }] });
  };

  const deleteProfile = (profile: OrchestratorProfile) => {
    const current = configRef.current;
    if (!current) return;
    if (assignedRolesForProfile(profile.id, current.assignments).length > 0) return;
    const nextProfiles = current.profiles.filter((item) => item.id !== profile.id);
    const nextQuickSlots = current.quickSlots.filter((slot) => slot.profileId !== profile.id);
    ollamaRequestGenerationRef.current.set(profile.id, (ollamaRequestGenerationRef.current.get(profile.id) ?? 0) + 1);
    void savePatch({ profiles: nextProfiles, quickSlots: nextQuickSlots });
  };

  const addProfile = (groupKey: string) => {
    const current = configRef.current;
    if (!current) return;
    const profile = defaultProfileForGroup(groupKey);
    toggleGroup(groupKey, true);
    void savePatch({ profiles: [...current.profiles, profile] });
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
    if (!current) return;
    if (patch.profileId && current.quickSlots.some((item) => item.id !== slot.id && item.profileId === patch.profileId)) return;
    void savePatch({ quickSlots: current.quickSlots.map((item) => item.id === slot.id ? { ...item, ...patch } : item) });
  };

  const refreshOllamaModels = async (profile: OrchestratorProfile) => {
    if (profile.adapter !== "ollama") return;
    const profileId = profile.id;
    const endpoint = profile.ollamaEndpoint?.trim() || undefined;
    const requestGeneration = (ollamaRequestGenerationRef.current.get(profileId) ?? 0) + 1;
    ollamaRequestGenerationRef.current.set(profileId, requestGeneration);
    setOllamaRefreshing((current) => new Set(current).add(profileId));
    setOllamaModelStatus((current) => ({ ...current, [profileId]: "" }));
    try {
      const response = await invoke<OllamaModelListResponse>("list_ollama_models_for_settings", {
        endpoint,
      });
      const currentProfile = configRef.current?.profiles.find((profile) => profile.id === profileId);
      const currentEndpoint = currentProfile?.ollamaEndpoint?.trim() || undefined;
      if (!aliveRef.current || requestGeneration !== ollamaRequestGenerationRef.current.get(profileId) || !currentProfile || currentEndpoint !== endpoint) return;
      if (response.status === "success") {
        setOllamaModels((current) => ({ ...current, [profileId]: response.models }));
        setOllamaModelStatus((current) => ({ ...current, [profileId]: response.models.length > 0
          ? `${response.models.length} ${t("apiKeyPanel.ollamaLocal.modelsFound")}`
          : t("apiKeyPanel.ollamaLocal.noModelsFound") }));
      } else {
        setOllamaModelStatus((current) => ({ ...current, [profileId]: t(`apiKeyPanel.ollamaLocal.error.${response.code}`) }));
      }
    } catch {
      if (aliveRef.current && requestGeneration === ollamaRequestGenerationRef.current.get(profileId)) {
        setOllamaModelStatus((current) => ({ ...current, [profileId]: t("apiKeyPanel.ollamaLocal.error.invalid_response") }));
      }
    } finally {
      if (aliveRef.current && requestGeneration === ollamaRequestGenerationRef.current.get(profileId)) {
        setOllamaRefreshing((current) => {
          const next = new Set(current);
          next.delete(profileId);
          return next;
        });
      }
    }
  };

  return (
    <div className="orchestrator-settings-panel">
      {error && <p role="alert">{error}</p>}
      <p aria-live="polite">{saving ? t("orchestrator.settings.saving") : ""}</p>

      <section className="orchestrator-settings-section orchestrator-profile-management">
        <h3>{t("orchestrator.settings.profiles")}</h3>
        <div className="orchestrator-profile-groups">
          {ORCHESTRATOR_PROVIDER_GROUPS.map((group) => {
            const profiles = config.profiles.filter((profile) => inferProfileGroupKey(profile) === group.key);
            const expanded = expandedGroups.has(group.key);
            return (
              <details
                className="orchestrator-profile-group"
                data-provider={group.key}
                key={group.key}
                open={expanded}
                onToggle={(event) => toggleGroup(group.key, event.currentTarget.open)}
              >
                <summary>
                  <span className="orchestrator-profile-group-chevron" aria-hidden="true">{expanded ? "▾" : "▸"}</span>
                  <span className="orchestrator-profile-group-name">{group.name}</span>
                  <span className="orchestrator-profile-group-summary">
                    {profiles.length > 0 ? profileGroupSummary(profiles[0]) : ""}
                  </span>
                  {profiles.length > 1 && (
                    <span className="orchestrator-profile-count">
                      {profiles.length} {t("orchestrator.settings.profiles")}
                    </span>
                  )}
                </summary>
                <div className="orchestrator-profile-group-body">
                  {profiles.map((profile) => {
                    const slots = config.quickSlots.filter((slot) => slot.profileId === profile.id);
                    const profileDisplayName = getOrchestratorProfileDisplayName(profile);
                    const visible = slots.some((slot) => slot.visible);
                    const assignedRoles = assignedRolesForProfile(profile.id, config.assignments);
                    const isProvider = profile.adapter === "provider";
                    const models = isProvider ? [...getProviderModels(profile.providerId ?? "")] : [];
                    if (profile.model && !models.includes(profile.model)) models.push(profile.model);
                    const modelPolicy = profileModelPolicy(profile);
                    const modelOptions = modelPolicy.options;
                    const currentModelList = ollamaModels[profile.id] ?? [];
                    const customModelEditing = ollamaCustomModelEditing.has(profile.id);
                    return (
                      <article className={`orchestrator-profile-card ${profiles.length > 1 ? "multi-profile" : ""}`} data-profile-id={profile.id} key={profile.id}>
                        <div className={`orchestrator-profile-header ${profiles.length === 1 && assignedRoles.length === 0 ? "actions-only" : ""}`}>
                          <div className="orchestrator-profile-identity-group">
                            {profiles.length > 1 && (
                              <div className="orchestrator-profile-name">{profileDisplayName}</div>
                            )}
                            {assignedRoles.length > 0 && (
                              <div className="orchestrator-profile-assignments" role="status">
                                {assignedRoles.map((role) => (
                                  <span key={role} className="orchestrator-profile-assignment-chip">
                                    {t(ROLE_NAME_KEYS[role] ?? `orchestrator.roles.${role}`)}
                                  </span>
                                ))}
                              </div>
                            )}
                          </div>
                          <button
                            type="button"
                            className="orchestrator-btn-delete"
                            disabled={saving || assignedRoles.length > 0}
                            title={assignedRoles.length ? assignedRoles.map((role) => t(ROLE_NAME_KEYS[role] ?? `orchestrator.roles.${role}`)).join(", ") : undefined}
                            onClick={() => deleteProfile(profile)}
                          >
                            {t("orchestrator.settings.deleteProfile")}
                          </button>
                        </div>
                        <div className="orchestrator-profile-controls-row">
                          {isProvider && (
                            <>
                              <label className="orchestrator-control-item">
                                <span className="orchestrator-control-label">{t("orchestrator.settings.model")}</span>
                                {models.length > 0 ? (
                                  <select className="orchestrator-select" value={profile.model ?? ""} onChange={(event) => updateModel(profile, event.target.value)}>
                                    {!profile.model && <option value="">—</option>}
                                    {models.map((model) => (
                                      <option key={model} value={model}>
                                        {profile.providerId === "openrouter" ? getOpenRouterModelDisplayName(model) : getModelDisplayName(model, profile.providerId)}
                                      </option>
                                    ))}
                                  </select>
                                ) : (
                                  <input className="orchestrator-input" value={profile.model ?? ""} onChange={(event) => updateModel(profile, event.target.value)} />
                                )}
                              </label>
                              {modelPolicy.policy === "thinking_only" ? (
                                <div className="orchestrator-control-item">
                                  <span className="orchestrator-control-label">{t("orchestrator.settings.thinkingMode")}</span>
                                  <span className="orchestrator-control-value-muted">{t("apiKeyPanel.thinkingOnly")}</span>
                                </div>
                              ) : modelPolicy.policy !== "none" ? (
                                <label className="orchestrator-control-item">
                                  <span className="orchestrator-control-label">{t("orchestrator.settings.thinkingMode")}</span>
                                  <select
                                    className="orchestrator-select"
                                    value={modelPolicy.policy === "forced" ? "thinking" : profile.thinkingMode ?? "thinking"}
                                    disabled={modelPolicy.policy === "forced"}
                                    onChange={(event) => updateThinkingMode(profile, event.target.value as "normal" | "thinking")}
                                  >
                                    <option value="thinking">Thinking</option>
                                    <option value="normal">Normal</option>
                                  </select>
                                </label>
                              ) : null}
                              {modelOptions.length > 0 && (profile.thinkingMode === "thinking" || modelPolicy.policy === "thinking_only" || modelPolicy.policy === "forced") && (
                                <label className="orchestrator-control-item">
                                  <span className="orchestrator-control-label">{t("orchestrator.settings.reasoningEffort")}</span>
                                  <select className="orchestrator-select" value={profile.reasoningEffort ?? modelPolicy.forcedEffort ?? ""} onChange={(event) => updateProfile(profile.id, { reasoningEffort: event.target.value || undefined })}>
                                    {modelOptions.map((option) => <option key={option} value={option}>{option.toUpperCase()}</option>)}
                                  </select>
                                </label>
                              )}
                              {profile.reasoningEffort && modelOptions.length === 0 && (
                                <label className="orchestrator-control-item">
                                  <span className="orchestrator-control-label">{t("orchestrator.settings.reasoningEffort")}</span>
                                  <input className="orchestrator-input" value={profile.reasoningEffort} onChange={(event) => updateProfile(profile.id, { reasoningEffort: event.target.value || undefined })} />
                                </label>
                              )}
                            </>
                          )}
                          {profile.adapter === "ollama" && (
                            <>
                              <label className="orchestrator-control-item">
                                <span className="orchestrator-control-label">{t("orchestrator.settings.endpoint")}</span>
                                <input
                                  className="orchestrator-input"
                                  value={profile.ollamaEndpoint ?? "http://127.0.0.1:11434"}
                                  onChange={(event) => {
                                    ollamaRequestGenerationRef.current.set(profile.id, (ollamaRequestGenerationRef.current.get(profile.id) ?? 0) + 1);
                                    setOllamaRefreshing((current) => { const next = new Set(current); next.delete(profile.id); return next; });
                                    setOllamaModels((current) => ({ ...current, [profile.id]: [] }));
                                    setOllamaModelStatus((current) => ({ ...current, [profile.id]: "" }));
                                    updateProfile(profile.id, { ollamaEndpoint: event.target.value });
                                  }}
                                />
                              </label>
                              <div className="orchestrator-control-item">
                                <span className="orchestrator-control-label">{t("orchestrator.settings.model")}</span>
                                {customModelEditing ? (
                                  <span className="orchestrator-inline-control">
                                    <input className="orchestrator-input" aria-label={`${profileDisplayName} ${t("apiKeyPanel.ollamaLocal.customModel")}`} value={profile.ollamaModel ?? ""} onChange={(event) => updateProfile(profile.id, { ollamaModel: event.target.value })} />
                                    <button type="button" className="orchestrator-btn-icon" aria-label={t("apiKeyPanel.ollamaLocal.selectInstalledModel")} onClick={() => setOllamaCustomModelEditing((current) => { const next = new Set(current); next.delete(profile.id); return next; })}>☷</button>
                                  </span>
                                ) : (
                                  <select
                                    className="orchestrator-select"
                                    aria-label={`${profileDisplayName} ${t("apiKeyPanel.ollamaLocal.modelTag")}`}
                                    value={profile.ollamaModel ?? ""}
                                    onChange={(event) => {
                                      if (event.target.value === "__custom_model__") setOllamaCustomModelEditing((current) => new Set(current).add(profile.id));
                                      else if (event.target.value) updateProfile(profile.id, { ollamaModel: event.target.value });
                                    }}
                                  >
                                    {!profile.ollamaModel && <option value="">{t("apiKeyPanel.ollamaLocal.selectInstalledModel")}</option>}
                                    {profile.ollamaModel && !currentModelList.includes(profile.ollamaModel) && <option value={profile.ollamaModel}>{profile.ollamaModel}</option>}
                                    {currentModelList.map((model) => <option key={model} value={model}>{model}</option>)}
                                    <option value="__custom_model__">{t("apiKeyPanel.ollamaLocal.customModel")}</option>
                                  </select>
                                )}
                                <button type="button" className="orchestrator-btn-icon" disabled={ollamaRefreshing.has(profile.id)} aria-label={t("apiKeyPanel.ollamaLocal.refresh")} onClick={() => void refreshOllamaModels(profile)}>
                                  {ollamaRefreshing.has(profile.id) ? "…" : "↻"}
                                </button>
                              </div>
                              {ollamaModelStatus[profile.id] && <span className="orchestrator-profile-status" role="status">{ollamaModelStatus[profile.id]}</span>}
                            </>
                          )}
                          {profile.adapter === "cli" && (
                            <>
                              <label className="orchestrator-control-item">
                                <span className="orchestrator-control-label">{t("orchestrator.settings.executable")}</span>
                                <input className="orchestrator-input" value={profile.executable ?? "codex"} onChange={(event) => updateProfile(profile.id, { executable: event.target.value })} />
                              </label>
                              <label className="orchestrator-control-item">
                                <span className="orchestrator-control-label">{t("orchestrator.settings.arguments")}</span>
                                <input className="orchestrator-input" value={(profile.args ?? []).join(" ")} onChange={(event) => updateProfile(profile.id, { args: event.target.value.split(/\s+/).filter(Boolean) })} />
                              </label>
                            </>
                          )}
                          <label className="orchestrator-profile-visible">
                            <input type="checkbox" checked={visible} onChange={(event) => setProfileVisible(profile, event.target.checked)} />
                            {t("orchestrator.settings.visible")}
                          </label>
                        </div>
                        <div
                          className="orchestrator-profile-capabilities-row"
                          role="group"
                          aria-label={t("orchestrator.settings.capabilities")}
                        >
                          <span className="orchestrator-control-label">{t("orchestrator.settings.capabilities")}</span>
                          <div className="orchestrator-capabilities-list">
                            {CAPABILITIES.map((capability) => (
                              <label key={capability} className="orchestrator-capability-checkbox">
                                <input type="checkbox" checked={profile.capabilities.includes(capability)} onChange={(event) => {
                                  const capabilities = event.target.checked
                                    ? [...new Set([...profile.capabilities, capability])]
                                    : profile.capabilities.filter((item) => item !== capability);
                                  updateProfile(profile.id, { capabilities });
                                }} />
                                {t(`orchestrator.capability.${capability}`)}
                              </label>
                            ))}
                          </div>
                        </div>
                        <details className="orchestrator-profile-advanced-details">
                          <summary className="orchestrator-profile-advanced-summary">
                            {t("apiKeyPanel.ollamaLocal.advancedSettings")}
                          </summary>
                          <div className="orchestrator-profile-advanced-body">
                            <label className="orchestrator-control-item">
                              <span className="orchestrator-control-label">{t("orchestrator.settings.contextWindow")}</span>
                              <input
                                className="orchestrator-input orchestrator-input-number"
                                type="number"
                                min={1}
                                step={1}
                                value={profile.contextWindowTokens ?? ""}
                                onChange={(event) => {
                                  const value = event.target.value;
                                  if (!value) updateProfile(profile.id, { contextWindowTokens: undefined });
                                  else if (/^\d+$/.test(value) && Number(value) > 0) updateProfile(profile.id, { contextWindowTokens: Number(value) });
                                }}
                              />
                            </label>
                          </div>
                        </details>
                      </article>
                    );
                  })}
                  <button type="button" disabled={saving} className="orchestrator-add-profile" onClick={() => addProfile(group.key)}>
                    + {t("orchestrator.settings.addProfile")} · {group.name}
                  </button>
                </div>
              </details>
            );
          })}
        </div>
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
