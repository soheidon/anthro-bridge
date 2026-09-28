import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../../i18n";
import type { AuthorizedCustomGate, ValidationGateConfig } from "../../types/orchestrator";

interface ValidationGatePanelProps {
  gates: ValidationGateConfig[];
  projectPath?: string;
  onChangeGates: (gates: ValidationGateConfig[]) => void;
}

export const ValidationGatePanel: React.FC<ValidationGatePanelProps> = ({
  gates,
  projectPath,
  onChangeGates,
}) => {
  const { t } = useTranslation();
  const [showAddForm, setShowAddForm] = useState(false);
  const [newName, setNewName] = useState("");
  const [newExecutable, setNewExecutable] = useState("");
  const [newArgs, setNewArgs] = useState("");
  const [newFailOnError, setNewFailOnError] = useState(true);
  const [authorizingId, setAuthorizingId] = useState<string | null>(null);

  const handleToggleGate = (id: string) => {
    const next = gates.map((g) => (g.id === id ? { ...g, enabled: !g.enabled } : g));
    onChangeGates(next);
  };

  const handleToggleFailOnError = (id: string) => {
    const next = gates.map((g) => (g.id === id ? { ...g, failOnError: !g.failOnError } : g));
    onChangeGates(next);
  };

  const handleRemoveGate = (id: string) => {
    const next = gates.filter((g) => g.id !== id);
    onChangeGates(next);
  };

  const handleAuthorizeGate = async (gate: ValidationGateConfig) => {
    if (!projectPath) return;
    setAuthorizingId(gate.id);
    try {
      await invoke<AuthorizedCustomGate>("authorize_custom_validation_gate", {
        gateId: gate.id,
        executable: gate.executable,
        args: gate.args,
        workingDir: gate.workingDir || null,
        projectRoot: projectPath,
      });
      alert(`Gate '${gate.name}' successfully authorized by backend!`);
    } catch (e: any) {
      alert(`Failed to authorize gate: ${e?.message || e}`);
    } finally {
      setAuthorizingId(null);
    }
  };

  const handleAddCustomGate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newExecutable.trim()) return;

    const argsArray = newArgs
      .trim()
      .split(/\s+/)
      .filter((a) => a.length > 0);

    const gateId = `custom-${Date.now()}`;
    const newGate: ValidationGateConfig = {
      id: gateId,
      name: newName.trim(),
      executable: newExecutable.trim(),
      args: argsArray,
      enabled: true,
      failOnError: newFailOnError,
      isAdvancedCustom: true,
    };

    // If project path is present, authorize gate on backend
    if (projectPath) {
      try {
        await invoke<AuthorizedCustomGate>("authorize_custom_validation_gate", {
          gateId,
          executable: newGate.executable,
          args: newGate.args,
          workingDir: null,
          projectRoot: projectPath,
        });
      } catch (err) {
        console.warn("Backend authorization will be required before running:", err);
      }
    }

    onChangeGates([...gates, newGate]);
    setNewName("");
    setNewExecutable("");
    setNewArgs("");
    setNewFailOnError(true);
    setShowAddForm(false);
  };

  return (
    <div className="orchestrator-card orchestrator-validation-gates">
      <div className="orchestrator-card-header">
        <h3 className="orchestrator-card-title">
          {t("orchestrator.gates.title") || "Deterministic Validation Gates"}
        </h3>
        <button
          type="button"
          className="orchestrator-btn orchestrator-btn-sm"
          onClick={() => setShowAddForm(!showAddForm)}
        >
          {showAddForm ? (t("common.cancel") || "Cancel") : `+ ${t("orchestrator.gates.addCustom") || "Add Gate"}`}
        </button>
      </div>

      <p className="orchestrator-desc">
        {t("orchestrator.gates.desc") ||
          "Direct process commands executed after implementation or fixes. Custom gates require explicit authorization."}
      </p>

      <div className="orchestrator-gates-list">
        {gates.map((gate) => (
          <div key={gate.id} className={`orchestrator-gate-item ${gate.enabled ? "enabled" : "disabled"}`}>
            <div className="gate-left">
              <label className="gate-toggle-label">
                <input
                  type="checkbox"
                  checked={gate.enabled}
                  onChange={() => handleToggleGate(gate.id)}
                />
                <span className="gate-name">{gate.name}</span>
              </label>
              <span className="gate-command">
                <code>
                  {gate.executable} {gate.args.join(" ")}
                </code>
              </span>
            </div>

            <div className="gate-right">
              {gate.isAdvancedCustom && projectPath && (
                <button
                  type="button"
                  className="orchestrator-btn orchestrator-btn-xs"
                  onClick={() => handleAuthorizeGate(gate)}
                  disabled={authorizingId === gate.id}
                  title="Authorize command on backend"
                >
                  {authorizingId === gate.id ? "Authorizing..." : "Authorize"}
                </button>
              )}

              <label className="gate-fail-toggle" title="Halt or trigger fix loop on failure">
                <input
                  type="checkbox"
                  checked={gate.failOnError}
                  disabled={!gate.enabled}
                  onChange={() => handleToggleFailOnError(gate.id)}
                />
                <span>{t("orchestrator.gates.failOnError") || "Required"}</span>
              </label>

              {gate.isAdvancedCustom && (
                <button
                  type="button"
                  className="orchestrator-btn-remove"
                  onClick={() => handleRemoveGate(gate.id)}
                  title="Remove Gate"
                >
                  ×
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      {showAddForm && (
        <form className="orchestrator-add-gate-form" onSubmit={handleAddCustomGate}>
          <div className="form-row">
            <input
              type="text"
              placeholder="Gate Name (e.g. Lint Check)"
              className="orchestrator-input"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              required
            />
            <input
              type="text"
              placeholder="Executable (e.g. npm, cargo, git)"
              className="orchestrator-input"
              value={newExecutable}
              onChange={(e) => setNewExecutable(e.target.value)}
              required
            />
          </div>
          <div className="form-row">
            <input
              type="text"
              placeholder="Arguments (e.g. run lint, test -- --run)"
              className="orchestrator-input"
              value={newArgs}
              onChange={(e) => setNewArgs(e.target.value)}
            />
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={newFailOnError}
                onChange={(e) => setNewFailOnError(e.target.checked)}
              />
              <span>{t("orchestrator.gates.failOnError") || "Required Gate"}</span>
            </label>
            <button type="submit" className="orchestrator-btn orchestrator-btn-primary">
              {t("orchestrator.gates.saveGate") || "Save Gate"}
            </button>
          </div>
        </form>
      )}
    </div>
  );
};
