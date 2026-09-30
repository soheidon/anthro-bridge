import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectSelector } from "./ProjectSelector";
import { translations as enTranslations } from "../../i18n/lang/en";
import { translations as jaTranslations } from "../../i18n/lang/ja";
import { translations as deTranslations } from "../../i18n/lang/de";
import { translations as esTranslations } from "../../i18n/lang/es";
import { translations as frTranslations } from "../../i18n/lang/fr";
import { translations as koTranslations } from "../../i18n/lang/ko";
import { translations as zhCNTranslations } from "../../i18n/lang/zh-CN";
import { translations as zhTWTranslations } from "../../i18n/lang/zh-TW";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

describe("ProjectSelector path pickers", () => {
  beforeEach(() => invokeMock.mockReset());

  it("fills the project path from the selected folder", async () => {
    invokeMock.mockResolvedValue("C:\\work\\sample");
    const onProjectPathChange = vi.fn();

    render(
      <ProjectSelector
        projectPath=""
        onProjectPathChange={onProjectPathChange}
        metadata={null}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        archiveDirectory=""
        nextArchiveFileName={null}
        archivePreviewError={null}
        showPlanArchive={false}
        onArchiveDirectoryChange={vi.fn()}
      />,
    );

    fireEvent.click(screen.getAllByRole("button", { name: "orchestrator.project.selectFolder" })[0]);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("select_project_folder_dialog");
      expect(onProjectPathChange).toHaveBeenCalledWith("C:\\work\\sample");
      expect(screen.getByRole("textbox")).toHaveValue("C:\\work\\sample");
    });
  });

  it("selects an alternate archive folder from the project directory", async () => {
    invokeMock.mockResolvedValue("C:\\work\\sample\\docs\\plans");
    const onArchiveDirectoryChange = vi.fn();
    render(
      <ProjectSelector
        projectPath={"C:\\work\\sample"}
        onProjectPathChange={vi.fn()}
        metadata={null}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        archiveDirectory={"C:\\work\\sample\\.plan"}
        nextArchiveFileName="V0.24.0-r12.md"
        archivePreviewError={null}
        showPlanArchive
        onArchiveDirectoryChange={onArchiveDirectoryChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.project.selectPlanFile" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("select_orchestrator_archive_folder_dialog", {
        projectPath: "C:\\work\\sample",
      });
      expect(onArchiveDirectoryChange).toHaveBeenCalledWith("C:\\work\\sample\\docs\\plans");
    });
    expect(screen.getByText("V0.24.0-r12.md")).toBeInTheDocument();
  });

  it("leaves the archive folder unchanged when the folder dialog is cancelled", async () => {
    invokeMock.mockResolvedValue(null);
    const onArchiveDirectoryChange = vi.fn();
    render(
      <ProjectSelector
        projectPath={"C:\\work\\sample"}
        onProjectPathChange={vi.fn()}
        metadata={null}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        archiveDirectory={"C:\\work\\sample\\.plan"}
        nextArchiveFileName={null}
        archivePreviewError={null}
        showPlanArchive
        onArchiveDirectoryChange={onArchiveDirectoryChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.project.selectPlanFile" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(onArchiveDirectoryChange).not.toHaveBeenCalled();
  });

  it("renders suggested validation gates and triggers onApplySuggestedGates", async () => {
    const onApplySuggestedGates = vi.fn();
    const suggestedGates = [
      {
        id: "npm_test",
        name: "Test (npm test)",
        category: "tests" as const,
        executable: "npm",
        args: ["test", "--", "--run"],
        enabled: true,
        failOnError: true,
      },
      {
        id: "tsc_check",
        name: "Type Check (tsc)",
        category: "static_check" as const,
        executable: "npx",
        args: ["tsc", "--noEmit"],
        enabled: true,
        failOnError: true,
      },
    ];

    render(
      <ProjectSelector
        projectPath="C:\\work\\sample"
        onProjectPathChange={vi.fn()}
        metadata={{
          path: "C:\\work\\sample",
          exists: true,
          isDirectory: true,
          projectType: "Node.js (TypeScript)",
          detectedFiles: {
            specMd: true,
            implementationPlanMd: true,
            agentsMd: false,
            readmeMd: true,
            cargoToml: false,
            packageJson: true,
            tsconfigJson: true,
            pyprojectToml: false,
            requirementsTxt: false,
            goMod: false,
            description: false,
            renvLock: false,
            claspJson: false,
            git: true,
          },
          suggestedGates,
        }}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        archiveDirectory=""
        nextArchiveFileName={null}
        archivePreviewError={null}
        showPlanArchive={false}
        onArchiveDirectoryChange={vi.fn()}
        onApplySuggestedGates={onApplySuggestedGates}
      />,
    );

    expect(screen.getByText("package.json ✓")).toBeInTheDocument();
    expect(screen.getByText("tsconfig.json ✓")).toBeInTheDocument();
    expect(screen.getByText("Git Repository ✓")).toBeInTheDocument();
    expect(screen.getByText("npm test -- --run")).toBeInTheDocument();
    expect(screen.getByText("npx tsc --noEmit")).toBeInTheDocument();

    const applyBtn = screen.getByRole("button", { name: "orchestrator.project.applySuggestedGates" });
    fireEvent.click(applyBtn);

    expect(onApplySuggestedGates).toHaveBeenCalledWith(suggestedGates);
  });

  it("hides manual apply button when autoValidationEnabled is true", () => {
    const suggestedGates = [
      {
        id: "npm_test",
        name: "Test (npm test)",
        category: "tests" as const,
        executable: "npm",
        args: ["test", "--", "--run"],
        enabled: true,
        failOnError: true,
      },
    ];

    render(
      <ProjectSelector
        projectPath="C:\\work\\sample"
        onProjectPathChange={vi.fn()}
        metadata={{
          path: "C:\\work\\sample",
          exists: true,
          isDirectory: true,
          projectType: "Node.js (TypeScript)",
          detectedFiles: {
            specMd: true,
            implementationPlanMd: true,
            agentsMd: false,
            readmeMd: true,
            cargoToml: false,
            packageJson: true,
            tsconfigJson: true,
            pyprojectToml: false,
            requirementsTxt: false,
            goMod: false,
            description: false,
            renvLock: false,
            claspJson: false,
            git: true,
          },
          suggestedGates,
        }}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        archiveDirectory=""
        nextArchiveFileName={null}
        archivePreviewError={null}
        showPlanArchive={false}
        onArchiveDirectoryChange={vi.fn()}
        onApplySuggestedGates={vi.fn()}
        autoValidationEnabled={true}
      />,
    );

    expect(screen.queryByRole("button", { name: "orchestrator.project.applySuggestedGates" })).not.toBeInTheDocument();
  });

  it("verifies suggested gates label is translated across all 8 locales in dictionary and renders translated text without raw key", () => {
    const locales = [
      ["en", enTranslations, "Detected Validation Gates"],
      ["ja", jaTranslations, "検出された検証項目"],
      ["de", deTranslations, "Erkannte Validierungsschritte"],
      ["es", esTranslations, "Validaciones detectadas"],
      ["fr", frTranslations, "Validations détectées"],
      ["ko", koTranslations, "감지된 검증 항목"],
      ["zh-CN", zhCNTranslations, "检测到的验证项目"],
      ["zh-TW", zhTWTranslations, "偵測到的驗證項目"],
    ] as const;

    const suggestedGates = [
      {
        id: "npm_test",
        name: "Test",
        category: "tests" as const,
        executable: "npm",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    for (const [code, dict, expected] of locales) {
      // 1. Dictionary assertion
      expect(dict["orchestrator.project.suggestedGatesLabel"], `Locale ${code}`).toBe(expected);
      expect(dict["orchestrator.project.suggestedGatesLabel"]).not.toBe("orchestrator.project.suggestedGatesLabel");

      // 2. Rendered DOM assertion
      const { unmount } = render(
        <ProjectSelector
          projectPath="C:\\work\\sample"
          onProjectPathChange={vi.fn()}
          metadata={{
            path: "C:\\work\\sample",
            exists: true,
            isDirectory: true,
            projectType: "Node.js (TypeScript)",
            detectedFiles: {
              specMd: false,
              implementationPlanMd: false,
              agentsMd: false,
              readmeMd: false,
              cargoToml: false,
              packageJson: true,
              tsconfigJson: false,
              pyprojectToml: false,
              requirementsTxt: false,
              goMod: false,
              description: false,
              renvLock: false,
              claspJson: false,
              git: false,
            },
            suggestedGates,
          }}
          onDetect={vi.fn(async () => {})}
          detecting={false}
          archiveDirectory=""
          nextArchiveFileName={null}
          archivePreviewError={null}
          showPlanArchive={false}
          onArchiveDirectoryChange={vi.fn()}
          t={(key) => dict[key as keyof typeof dict] ?? String(key)}
        />,
      );

      // Verify translated text is rendered and raw key is NEVER present
      expect(screen.getByText(expected), `Locale ${code} rendered text`).toBeInTheDocument();
      expect(screen.queryByText("orchestrator.project.suggestedGatesLabel")).not.toBeInTheDocument();

      unmount();
    }
  });
});
