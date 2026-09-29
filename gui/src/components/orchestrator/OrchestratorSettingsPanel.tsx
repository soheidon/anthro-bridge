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
import { getProviderModels, MODEL_CAPABILITIES } from "../../modelCapabilities";
import type { ModelCapabilities, ReasoningEffortOption, ThinkingModePolicy } from "../../modelCapabilities";
import { isReasoningEffortOption, normalizeReasoningEffort } from "../../reasoningEffort";

interface Props {
  t: (key: any) => string;
  onChanged?: () => Promise<void> | void;
}

const CAPABILITIES: ProfileCapability[] = ["reasoning", "review", "workspace_read", "workspace_write", "command_execution"];

const PROVIDER_GROUPS = [
  { key: "deepseek", name: "DeepSeek", providerId: "deepseek" },
  { key: "minimax", name: "MiniMax", providerId: "minimax" },
  { key: "kimi", name: "Kimi", providerId: "kimi" },
  { key: "kimi-code", name: "Kimi Code", providerId: "kimi-code" },
  { key: "mimo", name: "MiMo", providerId: "mimo" },
  { key: "openrouter", name: "OpenRouter", providerId: "openrouter" },
  { key: "ollama", name: "Ollama (Local)" },
  { key: "cli", name: "Codex CLI" },
  { key: "other", name: "Other" },
] as const;

const EXPANDED_GROUPS_KEY = "anthro-bridge.orchestrator-settings.expanded-providers";

function initialExpandedGroups(): Set<string> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(EXPANDED_GROUPS_KEY) ?? "[]");
    if (!Array.isArray(value)) return new Set();
    const allowed = new Set<string>(PROVIDER_GROUPS.map((group) => group.key));
    return new Set(value.filter((key): key is string => typeof key === "string" && allowed.has(key)));
  } catch {
    return new Set();
  }
}

function profileGroupKey(profile: OrchestratorProfile): string {
  if (profile.adapter === "ollama") return "ollama";
  if (profile.adapter === "cli") return "cli";
  if (profile.adapter === "provider") {
    return PROVIDER_GROUPS.some((group) => "providerId" in group && group.providerId === profile.providerId)
      ? profile.providerId ?? "other"
      : "other";
  }
  return "other";
}

function uniqueProfileId(): string {
  return `custom-${crypto.randomUUID()}`;
}

function defaultProfileForGroup(groupKey: string): OrchestratorProfile {
  const id = uniqueProfileId();
  const base = {
    id,
    displayName: "New profile",
    capabilities: ["reasoning", "review", "workspace_read"] as ProfileCapability[],
  };
  if (groupKey === "ollama") {
    return { ...base, displayName: "Ollama profile", adapter: "ollama", ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "", contextWindowTokens: 32768 };
  }
  if (groupKey === "cli") {
    return { ...base, displayName: "Codex CLI profile", adapter: "cli", executable: "codex", args: ["exec"], capabilities: [...base.capabilities, "workspace_write", "command_execution"], contextWindowTokens: 200000 };
  }
  if (groupKey === "other") {
    return { ...base, adapter: "provider", providerId: "custom", model: "custom-model", thinkingMode: "thinking", contextWindowTokens: 128000 };
  }
  const group = PROVIDER_GROUPS.find((item) => item.key === groupKey);
  const providerId = group && "providerId" in group ? group.providerId : "deepseek";
  const models = providerId === "openrouter" ? Object.keys(BUILTIN_OPENROUTER_MODELS) : getProviderModels(providerId);
  const preferredModel = providerId === "deepseek" ? "deepseek-v4.1-flash"
    : providerId === "mimo" ? "mimo-v2.6-pro"
      : providerId === "minimax" ? "MiniMax-M3"
        : providerId === "openrouter" ? "openai/gpt-5.6-sol"
          : models[0] ?? "";
  const model = models.includes(preferredModel) || providerId === "deepseek" ? preferredModel : models[0] ?? preferredModel;
  const displayModel = providerId === "openrouter"
    ? getOpenRouterModelDisplayName(model)
    : getModelDisplayName(model, providerId);
  const contextWindowTokens = providerId === "kimi" ? 1_048_576
    : providerId === "kimi-code" ? 200_000
      : providerId === "mimo" || providerId === "minimax" ? 1_000_000
        : providerId === "openrouter" ? 1_050_000
          : 128_000;
  const profile: OrchestratorProfile = {
    ...base,
    displayName: `${group?.name ?? providerId} ${displayModel}`,
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
  return profile;
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

function profilesWithoutQuickSlots(profiles: OrchestratorProfile[], quickSlots: OrchestratorQuickSlot[]): OrchestratorProfile[] {
  const assignedProfileIds = new Set(quickSlots.map((slot) => slot.profileId));
  return profiles.filter((profile) => !assignedProfileIds.has(profile.id));
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
    void savePatch({ profiles: current.profiles.map((profile) => profile.id === profileId ? { ...profile, ...patch } : profile) });
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
      label: profile.displayName,
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

  const removeSlot = (slotId: string) => {
    const current = configRef.current;
    if (current) void savePatch({ quickSlots: current.quickSlots.filter((item) => item.id !== slotId) });
  };

  const moveSlot = (slotId: string, direction: -1 | 1) => {
    const current = configRef.current;
    if (!current) return;
    const ordered = [...current.quickSlots].sort((left, right) => left.order - right.order);
    const index = ordered.findIndex((slot) => slot.id === slotId);
    const nextIndex = index + direction;
    if (index < 0 || nextIndex < 0 || nextIndex >= ordered.length) return;
    const currentOrder = ordered[index].order;
    ordered[index].order = ordered[nextIndex].order;
    ordered[nextIndex].order = currentOrder;
    void savePatch({ quickSlots: ordered });
  };

  const addSlot = () => {
    const current = configRef.current;
    if (!current) return;
    const availableProfile = profilesWithoutQuickSlots(current.profiles, current.quickSlots)[0];
    if (!availableProfile) return;
    const order = current.quickSlots.reduce((max, slot) => Math.max(max, slot.order), -1) + 1;
    void savePatch({ quickSlots: [...current.quickSlots, {
      id: `slot-${crypto.randomUUID()}`,
      profileId: availableProfile.id,
      label: t("orchestrator.settings.newSlot"),
      visible: true,
      order,
    }] });
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
      <h2>{t("orchestrator.settings.title")}</h2>
      {error && <p role="alert">{error}</p>}
      <p aria-live="polite">{saving ? t("orchestrator.settings.saving") : ""}</p>

      <section className="orchestrator-settings-section orchestrator-profile-management">
        <h3>{t("orchestrator.settings.profiles")}</h3>
        <div className="orchestrator-profile-groups">
          {PROVIDER_GROUPS.map((group) => {
            const profiles = config.profiles.filter((profile) => profileGroupKey(profile) === group.key);
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
                  <span>{group.name}</span>
                  <span className="orchestrator-profile-count">{profiles.length}</span>
                </summary>
                <div className="orchestrator-profile-group-body">
                  {profiles.map((profile) => {
                    const slots = config.quickSlots.filter((slot) => slot.profileId === profile.id);
                    const visible = slots.some((slot) => slot.visible);
                    const assignedRoles = assignedRolesForProfile(profile.id, config.assignments);
                    const isProvider = profile.adapter === "provider";
                    const models = isProvider
                      ? profile.providerId === "openrouter"
                        ? Object.keys(BUILTIN_OPENROUTER_MODELS)
                        : [...getProviderModels(profile.providerId ?? "")]
                      : [];
                    if (profile.model && !models.includes(profile.model)) models.push(profile.model);
                    const modelPolicy = profileModelPolicy(profile);
                    const modelOptions = modelPolicy.options;
                    const currentModelList = ollamaModels[profile.id] ?? [];
                    const customModelEditing = ollamaCustomModelEditing.has(profile.id);
                    return (
                      <article className="orchestrator-profile-card" data-profile-id={profile.id} key={profile.id}>
                        {assignedRoles.length > 0 && (
                          <p className="orchestrator-profile-assignment" role="status">
                            {assignedRoles.map((role) => t(`orchestrator.roles.${role}`)).join(", ")}
                          </p>
                        )}
                        <div className="orchestrator-profile-fields">
                          <label>
                            {t("orchestrator.settings.displayName")}
                            <input value={profile.displayName} onChange={(event) => updateProfile(profile.id, { displayName: event.target.value })} />
                          </label>
                          <label>
                            {t("orchestrator.settings.adapter")}
                            <select value={profile.adapter} onChange={(event) => updateProfile(profile.id, { adapter: event.target.value as OrchestratorProfile["adapter"] })}>
                              <option value="provider">Provider</option>
                              <option value="ollama">Ollama</option>
                              <option value="cli">CLI</option>
                              <option value="mcp">MCP</option>
                            </select>
                          </label>
                          {isProvider && (
                            <>
                              <label>
                                {t("orchestrator.settings.providerId")}
                                <select value={profile.providerId ?? ""} onChange={(event) => updateProfile(profile.id, { providerId: event.target.value })}>
                                  {PROVIDER_GROUPS.filter((item) => "providerId" in item).map((item) => (
                                    <option key={item.key} value={item.providerId}>{item.name}</option>
                                  ))}
                                  {profile.providerId && !PROVIDER_GROUPS.some((item) => "providerId" in item && item.providerId === profile.providerId) && (
                                    <option value={profile.providerId}>{profile.providerId}</option>
                                  )}
                                </select>
                              </label>
                              <label>
                                {t("orchestrator.settings.model")}
                                {models.length > 0 ? (
                                  <select value={profile.model ?? ""} onChange={(event) => updateModel(profile, event.target.value)}>
                                    {!profile.model && <option value="">—</option>}
                                    {models.map((model) => (
                                      <option key={model} value={model}>
                                        {profile.providerId === "openrouter" ? getOpenRouterModelDisplayName(model) : getModelDisplayName(model, profile.providerId)}
                                      </option>
                                    ))}
                                  </select>
                                ) : (
                                  <input value={profile.model ?? ""} onChange={(event) => updateModel(profile, event.target.value)} />
                                )}
                              </label>
                              <label>
                                {t("orchestrator.settings.thinkingMode")}
                                <select
                                  value={modelPolicy.policy === "thinking_only" || modelPolicy.policy === "forced" ? "thinking" : modelPolicy.policy === "none" ? "normal" : profile.thinkingMode ?? "thinking"}
                                  disabled={modelPolicy.policy === "thinking_only" || modelPolicy.policy === "forced" || modelPolicy.policy === "none"}
                                  onChange={(event) => updateThinkingMode(profile, event.target.value as "normal" | "thinking")}
                                >
                                  <option value="thinking">Thinking</option>
                                  <option value="normal">Normal</option>
                                </select>
                              </label>
                              {modelOptions.length > 0 && (profile.thinkingMode === "thinking" || modelPolicy.policy === "thinking_only" || modelPolicy.policy === "forced") && (
                                <label>
                                  {t("orchestrator.settings.reasoningEffort")}
                                  <select value={profile.reasoningEffort ?? modelPolicy.forcedEffort ?? ""} onChange={(event) => updateProfile(profile.id, { reasoningEffort: event.target.value || undefined })}>
                                    {modelOptions.map((option) => <option key={option} value={option}>{option.toUpperCase()}</option>)}
                                  </select>
                                </label>
                              )}
                              {profile.reasoningEffort && modelOptions.length === 0 && (
                                <label>
                                  {t("orchestrator.settings.reasoningEffort")}
                                  <input value={profile.reasoningEffort} onChange={(event) => updateProfile(profile.id, { reasoningEffort: event.target.value || undefined })} />
                                </label>
                              )}
                            </>
                          )}
                          {profile.adapter === "ollama" && (
                            <>
                              <label>
                                {t("orchestrator.settings.endpoint")}
                                <input
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
                              <label>
                                {t("orchestrator.settings.model")}
                                {customModelEditing ? (
                                  <span className="orchestrator-inline-control">
                                    <input aria-label={`${profile.displayName} ${t("apiKeyPanel.ollamaLocal.customModel")}`} value={profile.ollamaModel ?? ""} onChange={(event) => updateProfile(profile.id, { ollamaModel: event.target.value })} />
                                    <button type="button" aria-label={t("apiKeyPanel.ollamaLocal.selectInstalledModel")} onClick={() => setOllamaCustomModelEditing((current) => { const next = new Set(current); next.delete(profile.id); return next; })}>☷</button>
                                  </span>
                                ) : (
                                  <select
                                    aria-label={`${profile.displayName} ${t("apiKeyPanel.ollamaLocal.modelTag")}`}
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
                              </label>
                              <button type="button" disabled={ollamaRefreshing.has(profile.id)} aria-label={t("apiKeyPanel.ollamaLocal.refresh")} onClick={() => void refreshOllamaModels(profile)}>
                                {ollamaRefreshing.has(profile.id) ? "…" : "↻"}
                              </button>
                              {ollamaModelStatus[profile.id] && <span className="orchestrator-profile-status" role="status">{ollamaModelStatus[profile.id]}</span>}
                            </>
                          )}
                          {profile.adapter === "cli" && (
                            <>
                              <label>{t("orchestrator.settings.executable")}<input value={profile.executable ?? "codex"} onChange={(event) => updateProfile(profile.id, { executable: event.target.value })} /></label>
                              <label>{t("orchestrator.settings.arguments")}<input value={(profile.args ?? []).join(" ")} onChange={(event) => updateProfile(profile.id, { args: event.target.value.split(/\s+/).filter(Boolean) })} /></label>
                            </>
                          )}
                          <label>
                            {t("orchestrator.settings.contextWindow")}
                            <input type="number" min={1} step={1} value={profile.contextWindowTokens ?? ""} onChange={(event) => {
                              const value = event.target.value;
                              if (!value) updateProfile(profile.id, { contextWindowTokens: undefined });
                              else if (/^\d+$/.test(value) && Number(value) > 0) updateProfile(profile.id, { contextWindowTokens: Number(value) });
                            }} />
                          </label>
                          <label className="orchestrator-profile-visible">
                            <input type="checkbox" checked={visible} onChange={(event) => setProfileVisible(profile, event.target.checked)} />
                            {t("orchestrator.settings.visible")}
                          </label>
                        </div>
                        <fieldset className="orchestrator-profile-capabilities">
                          <legend>{t("orchestrator.settings.capabilities")}</legend>
                          {CAPABILITIES.map((capability) => (
                            <label key={capability}>
                              <input type="checkbox" checked={profile.capabilities.includes(capability)} onChange={(event) => {
                                const capabilities = event.target.checked
                                  ? [...new Set([...profile.capabilities, capability])]
                                  : profile.capabilities.filter((item) => item !== capability);
                                updateProfile(profile.id, { capabilities });
                              }} />
                              {t(`orchestrator.capability.${capability}`)}
                            </label>
                          ))}
                        </fieldset>
                        <div className="orchestrator-profile-actions">
                          <button
                            type="button"
                            disabled={saving || assignedRoles.length > 0}
                            title={assignedRoles.length ? assignedRoles.map((role) => t(`orchestrator.roles.${role}`)).join(", ") : undefined}
                            onClick={() => deleteProfile(profile)}
                          >
                            {t("orchestrator.settings.deleteProfile")}
                          </button>
                        </div>
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
        <h3>{t("orchestrator.settings.quickSlots")}</h3>
        {[...config.quickSlots].sort((left, right) => left.order - right.order).map((slot, index, ordered) => (
          <div className="orchestrator-settings-row orchestrator-quick-slot-row" key={slot.id} data-slot-id={slot.id}>
            <input aria-label={`${slot.id} ${t("orchestrator.settings.slotLabel")}`} value={slot.label} onChange={(event) => updateSlot(slot, { label: event.target.value })} />
            <select aria-label={`${slot.id} ${t("orchestrator.settings.slotProfile")}`} value={slot.profileId} onChange={(event) => updateSlot(slot, { profileId: event.target.value })}>
              {!config.profiles.some((profile) => profile.id === slot.profileId) && <option value={slot.profileId}>{t("orchestrator.settings.unavailableProfile")} ({slot.profileId})</option>}
              {config.profiles.filter((profile) => profile.id === slot.profileId || !config.quickSlots.some((other) => other.id !== slot.id && other.profileId === profile.id)).map((profile) => <option value={profile.id} key={profile.id}>{profile.displayName}</option>)}
            </select>
            <label><input type="checkbox" checked={slot.visible} onChange={(event) => updateSlot(slot, { visible: event.target.checked })} />{t("orchestrator.settings.visible")}</label>
            <button type="button" aria-label={`Move ${slot.label} up`} disabled={index === 0 || saving} onClick={() => moveSlot(slot.id, -1)}>↑</button>
            <button type="button" aria-label={`Move ${slot.label} down`} disabled={index === ordered.length - 1 || saving} onClick={() => moveSlot(slot.id, 1)}>↓</button>
            <button type="button" aria-label={`Remove ${slot.label}`} disabled={saving} onClick={() => removeSlot(slot.id)}>×</button>
          </div>
        ))}
        <button type="button" disabled={saving || profilesWithoutQuickSlots(config.profiles, config.quickSlots).length === 0} onClick={addSlot}>{t("orchestrator.settings.addSlot")}</button>
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
