import React, { useState, useCallback } from "react";
import { useTranslation } from "../../i18n";
import type { ProjectMetadataResponse } from "../../types/orchestrator";

interface ProjectSelectorProps {
  projectPath: string;
  onProjectPathChange: (path: string) => void;
  metadata: ProjectMetadataResponse | null;
  onDetect: (path?: string) => Promise<void>;
  detecting: boolean;
}

export const ProjectSelector: React.FC<ProjectSelectorProps> = ({
  projectPath,
  onProjectPathChange,
  metadata,
  onDetect,
  detecting,
}) => {
  const { t } = useTranslation();
  const [inputVal, setInputVal] = useState(projectPath);

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
          className="orchestrator-btn orchestrator-btn-primary"
          onClick={() => onDetect(inputVal)}
          disabled={detecting || !inputVal.trim()}
        >
          {detecting ? (t("orchestrator.project.detecting") || "Detecting...") : (t("orchestrator.project.detectBtn") || "Scan Project")}
        </button>
      </div>

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
