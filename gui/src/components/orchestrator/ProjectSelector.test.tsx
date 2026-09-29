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
});
