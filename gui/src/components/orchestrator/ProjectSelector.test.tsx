import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectSelector } from "./ProjectSelector";

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
        planFilePath=""
        showPlanFile={false}
        onPlanFilePathChange={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.project.selectFolder" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("select_project_folder_dialog");
      expect(onProjectPathChange).toHaveBeenCalledWith("C:\\work\\sample");
      expect(screen.getByRole("textbox")).toHaveValue("C:\\work\\sample");
    });
  });

  it("selects an alternate Markdown plan output path from the project directory", async () => {
    invokeMock.mockResolvedValue("C:\\work\\sample\\docs\\plan.md");
    const onPlanFilePathChange = vi.fn();
    render(
      <ProjectSelector
        projectPath={"C:\\work\\sample"}
        onProjectPathChange={vi.fn()}
        metadata={null}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        planFilePath={"C:\\work\\sample\\IMPLEMENTATION_PLAN.md"}
        showPlanFile
        onPlanFilePathChange={onPlanFilePathChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.project.selectPlanFile" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("select_orchestrator_plan_file_dialog", {
        projectPath: "C:\\work\\sample",
      });
      expect(onPlanFilePathChange).toHaveBeenCalledWith("C:\\work\\sample\\docs\\plan.md");
    });
  });

  it("leaves the plan path unchanged when the Save dialog is cancelled", async () => {
    invokeMock.mockResolvedValue(null);
    const onPlanFilePathChange = vi.fn();
    render(
      <ProjectSelector
        projectPath={"C:\\work\\sample"}
        onProjectPathChange={vi.fn()}
        metadata={null}
        onDetect={vi.fn(async () => {})}
        detecting={false}
        planFilePath={"C:\\work\\sample\\IMPLEMENTATION_PLAN.md"}
        showPlanFile
        onPlanFilePathChange={onPlanFilePathChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.project.selectPlanFile" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(onPlanFilePathChange).not.toHaveBeenCalled();
  });
});
