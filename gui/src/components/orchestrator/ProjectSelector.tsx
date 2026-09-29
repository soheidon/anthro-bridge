import React, { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../../i18n";
import type { ProjectMetadataResponse } from "../../types/orchestrator";

interface ProjectSelectorProps {
  projectPath: string;
  onProjectPathChange: (path: string) => void;
  metadata: ProjectMetadataResponse | null;
  onDetect: (path?: string) => Promise<void>;
  detecting: boolean;
  archiveDirectory: string;
  nextArchiveFileName: string | null;
  archivePreviewError: string | null;
  showPlanArchive: boolean;
  onArchiveDirectoryChange: (path: string) => void;
}

export const ProjectSelector: React.FC<ProjectSelectorProps> = ({
  projectPath,
  onProjectPathChange,
  metadata,
  onDetect,
  detecting,
  archiveDirectory,
  nextArchiveFileName,
  archivePreviewError,
  showPlanArchive,
  onArchiveDirectoryChange,
}) => {
  const { t } = useTranslation();
  const [inputVal, setInputVal] = useState(projectPath);
  const [archivePickerError, setArchivePickerError] = useState<string | null>(null);

  React.useEffect(() => {
    setInputVal(projectPath);
  }, [projectPath]);

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setInputVal(e.target.value);
    onProjectPathChange(e.target.value);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      void onDetect(inputVal);
    }
  };

  const handleBlur = () => {
    if (inputVal !== projectPath) {
      onProjectPathChange(inputVal);
    }
  };

  const handleSelectFolder = async () => {
    try {
      const selectedPath = await invoke<string | null>("select_project_folder_dialog");
      if (selectedPath) {
        setInputVal(selectedPath);
        onProjectPathChange(selectedPath);
      }
    } catch (error) {
      console.error("Failed to select project folder:", error);
    }
  };

  const handleSelectArchiveFolder = async () => {
    setArchivePickerError(null);
    try {
      const selectedPath = await invoke<string | null>("select_orchestrator_archive_folder_dialog", {
        projectPath,
      });
      if (selectedPath) onArchiveDirectoryChange(selectedPath);
    } catch (error) {
      setArchivePickerError(String(error));
    }
  };

  const detectedFiles = metadata?.detected_files || (metadata as any)?.detectedFiles;
  const projectType = metadata?.project_type || (metadata as any)?.projectType;

  const specMd = Boolean(detectedFiles?.spec_md ?? (detectedFiles as any)?.specMd);
  const implementationPlanMd = Boolean(detectedFiles?.implementation_plan_md ?? (detectedFiles as any)?.implementationPlanMd);
  const agentsMd = Boolean(detectedFiles?.agents_md ?? (detectedFiles as any)?.agentsMd);
  const readmeMd = Boolean(detectedFiles?.readme_md ?? (detectedFiles as any)?.readmeMd);
  const cargoToml = Boolean(detectedFiles?.cargo_toml ?? (detectedFiles as any)?.cargoToml);
  const packageJson = Boolean(detectedFiles?.package_json ?? (detectedFiles as any)?.packageJson);
  const pyprojectToml = Boolean(detectedFiles?.pyproject_toml ?? (detectedFiles as any)?.pyprojectToml);
  const git = Boolean(detectedFiles?.git);

  return (
    <div className="orchestrator-card orchestrator-project-selector">
      <div className="orchestrator-card-header">
        <h3 className="orchestrator-card-title">{t("orchestrator.project.title") || "Project Directory & Metadata"}</h3>
        <span className="orchestrator-badge-type">
          {metadata ? (metadata.exists ? projectType : "Not Found") : "Unchecked"}
        </span>
      </div>

      <div className="orchestrator-project-input-row">
        <input
          type="text"
          className="orchestrator-input orchestrator-path-input"
          placeholder="C:\path\to\project or /path/to/project"
          value={inputVal}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onBlur={handleBlur}
        />
        <button
          type="button"
          className="orchestrator-btn orchestrator-folder-picker-btn"
          onClick={() => void handleSelectFolder()}
          aria-label={t("orchestrator.project.selectFolder") || "Select project folder"}
          title={t("orchestrator.project.selectFolder") || "Select project folder"}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h5l2 2H19.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" />
            <path d="M3.5 9h17" />
          </svg>
        </button>
        <button
          type="button"
          className="orchestrator-btn orchestrator-btn-primary"
          onClick={() => onDetect(inputVal)}
          disabled={detecting || !inputVal.trim()}
        >
          {detecting ? (t("orchestrator.project.detecting") || "Detecting...") : (t("orchestrator.project.detectBtn") || "Scan Project")}
        </button>
      </div>

      {showPlanArchive && (
        <div className="orchestrator-project-input-row orchestrator-plan-file-row">
          <label className="orchestrator-plan-file-label" htmlFor="orchestrator-plan-archive-directory">
            {t("orchestrator.project.planFile") || "Plan archive folder"}
          </label>
          <input
            id="orchestrator-plan-archive-directory"
            type="text"
            className="orchestrator-input orchestrator-path-input"
            aria-label={t("orchestrator.project.planFile") || "Plan archive folder"}
            value={archiveDirectory}
            onChange={(event) => onArchiveDirectoryChange(event.target.value)}
          />
          <button
            type="button"
            className="orchestrator-btn orchestrator-folder-picker-btn"
            onClick={() => void handleSelectArchiveFolder()}
            aria-label={t("orchestrator.project.selectPlanFile") || "Select archive folder"}
            title={t("orchestrator.project.selectPlanFile") || "Select archive folder"}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h5l2 2H19.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" />
              <path d="M3.5 9h17" />
            </svg>
          </button>
          <span className="orchestrator-plan-archive-preview" aria-live="polite">
            {nextArchiveFileName ?? archivePreviewError ?? ""}
          </span>
        </div>
      )}
      {(archivePickerError || archivePreviewError) && <div className="orchestrator-error-notice">{archivePickerError || archivePreviewError}</div>}

      {metadata && metadata.exists && detectedFiles && (
        <div className="orchestrator-file-badges">
          <span className={`file-badge ${specMd ? "detected" : "missing"}`}>
            SPEC.md {specMd ? "✓" : "✗"}
          </span>
          <span className={`file-badge ${implementationPlanMd ? "detected" : "missing"}`}>
            IMPLEMENTATION_PLAN.md {implementationPlanMd ? "✓" : "✗"}
          </span>
          <span className={`file-badge ${agentsMd ? "detected" : "missing"}`}>
            AGENTS.md {agentsMd ? "✓" : "✗"}
          </span>
          <span className={`file-badge ${readmeMd ? "detected" : "missing"}`}>
            README.md {readmeMd ? "✓" : "✗"}
          </span>
          {cargoToml && <span className="file-badge detected">Cargo.toml ✓</span>}
          {packageJson && <span className="file-badge detected">package.json ✓</span>}
          {pyprojectToml && <span className="file-badge detected">pyproject.toml ✓</span>}
          {git && <span className="file-badge detected">Git Repository ✓</span>}
        </div>
      )}

      {metadata && !metadata.exists && (
        <div className="orchestrator-error-notice">
          {t("orchestrator.project.notFound") || "Specified directory does not exist. Please check the path."}
        </div>
      )}
    </div>
  );
};
