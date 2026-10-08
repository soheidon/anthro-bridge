import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { PlanWorkspaceSettings, formatPlanWorkspaceWarning } from "./PlanWorkspaceSettings";
import type {
  OrchestratorConfig,
  PlanAppendResponse,
  PlanContext,
  PlanNewConfirmResponse,
  PlanNewPreviewResponse,
} from "../../types/orchestrator";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const dummyConfig: OrchestratorConfig = {
  projectPath: "/repo",
  planWorkspace: {
    planDir: ".plan",
    filenameTemplate: "V{version}-r{revision}{suffix}.md",
    versionSources: ["package.json", "Cargo.toml"],
    planSeriesVersionOverride: "0.24.0",
  },
};

const resolvedContext: PlanContext = {
  projectRootIdentity: "/repo",
  planDirectory: ".plan",
  applicationVersion: "0.24.0",
  planSeriesVersion: "0.24.0",
  currentPrimaryPlan: {
    id: "V0.24.0-r22",
    path: ".plan/V0.24.0-r22.md",
    digest: "a1b2c3d4e5f6",
    revision: 22,
  },
  activeSupplementalPlans: [
    {
      id: "V0.24.0-r22a",
      path: ".plan/V0.24.0-r22a.md",
      digest: "f6e5d4c3b2a1",
      suffix: "a",
    },
  ],
  currentLeafPlanId: "V0.24.0-r22a",
  effectivePlanDigest: "11223344556677889900aabbccddeeff",
  currentPrimaryRevision: 22,
  nextPrimaryRevision: 23,
  resolverStatus: "resolved",
  unresolvedReasonCode: undefined,
};

const unresolvedContext: PlanContext = {
  projectRootIdentity: "/repo",
  planDirectory: ".plan",
  applicationVersion: "0.24.0",
  planSeriesVersion: "0.24.0",
  currentPrimaryPlan: undefined,
  activeSupplementalPlans: [],
  currentLeafPlanId: undefined,
  effectivePlanDigest: undefined,
  currentPrimaryRevision: undefined,
  nextPrimaryRevision: undefined,
  resolverStatus: "unresolved",
  unresolvedReasonCode: "orphaned_supplemental_plan",
};

describe("PlanWorkspaceSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders resolved plan status with primary, leaf, digest, and supplemental chain", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "get_plan_context") return resolvedContext;
      return null;
    });

    render(
      <PlanWorkspaceSettings
        config={dummyConfig}
        projectPath="/repo"
        onChange={vi.fn()}
        t={(key, params) => {
          if (params?.id) return `${key}:${params.id}`;
          return String(key);
        }}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("orchestrator.planWorkspace.resolved")).toBeInTheDocument();
    });

    expect(screen.getByText("V0.24.0-r22 (.plan/V0.24.0-r22.md)")).toBeInTheDocument();
    expect(screen.getAllByText("V0.24.0-r22a")).toHaveLength(2);
    expect(screen.getByText("11223344556677889900aabbccddeeff")).toBeInTheDocument();
  });

  it("renders unresolved plan status with localized reason code", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "get_plan_context") return unresolvedContext;
      return null;
    });

    render(
      <PlanWorkspaceSettings
        config={dummyConfig}
        projectPath="/repo"
        onChange={vi.fn()}
        t={(key) => String(key)}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("orchestrator.planWorkspace.unresolved")).toBeInTheDocument();
    });

    expect(
      screen.getByText("orchestrator.planWorkspace.reason.orphaned_supplemental_plan"),
    ).toBeInTheDocument();
  });

  it("handles preview -> cancel with ZERO mutations", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "get_plan_context") return resolvedContext;
      if (cmd === "preview_new_plan") {
        const preview: PlanNewPreviewResponse = {
          candidateRevision: 23,
          candidateFilename: "V0.24.0-r23.md",
          candidatePath: ".plan/V0.24.0-r23.md",
          token: "preview-hmac-token-12345",
          context: resolvedContext,
        };
        return preview;
      }
      return null;
    });

    render(
      <PlanWorkspaceSettings
        config={dummyConfig}
        projectPath="/repo"
        onChange={vi.fn()}
        t={(key) => String(key)}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("orchestrator.planWorkspace.resolved")).toBeInTheDocument();
    });

    // Click Preview New Plan
    fireEvent.click(screen.getByText("orchestrator.planWorkspace.previewNew"));

    await waitFor(() => {
      expect(screen.getByText("V0.24.0-r23.md")).toBeInTheDocument();
      expect(screen.getByText("r23")).toBeInTheDocument();
    });

    // Click Cancel in modal
    fireEvent.click(screen.getByText("orchestrator.planWorkspace.previewModal.cancelBtn"));

    // Verify confirm_new_plan was NEVER called
    expect(invokeMock).not.toHaveBeenCalledWith("confirm_new_plan", expect.anything());
    expect(screen.queryByText("V0.24.0-r23.md")).not.toBeInTheDocument();
  });

  it("handles preview -> confirm create plan lifecycle", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "get_plan_context") return resolvedContext;
      if (cmd === "preview_new_plan") {
        const preview: PlanNewPreviewResponse = {
          candidateRevision: 23,
          candidateFilename: "V0.24.0-r23.md",
          candidatePath: ".plan/V0.24.0-r23.md",
          token: "preview-hmac-token-12345",
          context: resolvedContext,
        };
        return preview;
      }
      if (cmd === "confirm_new_plan") {
        const confirmRes: PlanNewConfirmResponse = {
          createdPlanId: "V0.24.0-r23",
          createdPath: ".plan/V0.24.0-r23.md",
          fileDigest: "newdigest1234",
          context: {
            ...resolvedContext,
            currentPrimaryPlan: {
              id: "V0.24.0-r23",
              path: ".plan/V0.24.0-r23.md",
              digest: "newdigest1234",
              revision: 23,
            },
          },
        };
        return confirmRes;
      }
      return null;
    });

    render(
      <PlanWorkspaceSettings
        config={dummyConfig}
        projectPath="/repo"
        onChange={vi.fn()}
        t={(key, params) => {
          if (params?.id) return `${key}:${params.id}`;
          return String(key);
        }}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("orchestrator.planWorkspace.resolved")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("orchestrator.planWorkspace.previewNew"));

    await waitFor(() => {
      expect(screen.getByText("V0.24.0-r23.md")).toBeInTheDocument();
    });

    fireEvent.change(screen.getByPlaceholderText("Feature Implementation"), {
      target: { value: "New Feature Plan" },
    });
    fireEvent.change(screen.getByPlaceholderText(/Details.../), {
      target: { value: "# New Feature Plan\n\nPlan details here." },
    });

    fireEvent.click(screen.getByText("orchestrator.planWorkspace.previewModal.confirmBtn"));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("confirm_new_plan", {
        projectPath: "/repo",
        request: {
          token: "preview-hmac-token-12345",
          title: "New Feature Plan",
          initialContent: "# New Feature Plan\n\nPlan details here.",
        },
        config: dummyConfig.planWorkspace,
      });
    });
  });

  it("handles append plan section modal and submission", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "get_plan_context") return resolvedContext;
      if (cmd === "append_plan_section") {
        const appendRes: PlanAppendResponse = {
          planId: "V0.24.0-r22a",
          updatedFileDigest: "updateddigest999",
          applied: true,
          context: resolvedContext,
        };
        return appendRes;
      }
      return null;
    });

    render(
      <PlanWorkspaceSettings
        config={dummyConfig}
        projectPath="/repo"
        onChange={vi.fn()}
        t={(key, params) => {
          if (params?.id) return `${key}:${params.id}`;
          return String(key);
        }}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("orchestrator.planWorkspace.resolved")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("orchestrator.planWorkspace.appendSection"));

    await waitFor(() => {
      expect(screen.getByText("orchestrator.planWorkspace.appendModal.title")).toBeInTheDocument();
    });

    fireEvent.change(screen.getByPlaceholderText("Section Title"), {
      target: { value: "Review Findings" },
    });
    fireEvent.change(screen.getByPlaceholderText("Section markdown content..."), {
      target: { value: "All findings resolved successfully." },
    });

    fireEvent.click(screen.getByText("orchestrator.planWorkspace.appendModal.appendBtn"));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "append_plan_section",
        expect.objectContaining({
          projectPath: "/repo",
          request: expect.objectContaining({
            targetPlanId: "V0.24.0-r22a",
            sectionTitle: "Review Findings",
            sectionContent: "All findings resolved successfully.",
          }),
        }),
      );
    });
  });

  describe("formatPlanWorkspaceError and error UI", () => {
    it("localizes known error code from object payload without exposing raw backend English", async () => {
      invokeMock.mockImplementation(async (cmd) => {
        if (cmd === "get_plan_context") {
          throw {
            code: "stale_digest",
            message: "Target plan file digest mismatch on disk: expected abc, got def",
          };
        }
        return null;
      });

      render(
        <PlanWorkspaceSettings
          config={dummyConfig}
          projectPath="/repo"
          onChange={vi.fn()}
          t={(key) => {
            if (key === "orchestrator.planWorkspace.errors.stale_digest") {
              return "Localized: Plan file has been modified. Please refresh.";
            }
            return String(key);
          }}
        />,
      );

      await waitFor(() => {
        expect(
          screen.getByText("Localized: Plan file has been modified. Please refresh."),
        ).toBeInTheDocument();
      });

      expect(
        screen.queryByText(/Target plan file digest mismatch on disk/),
      ).not.toBeInTheDocument();
    });

    it("falls back to generic localized message for unknown error code without exposing raw English", async () => {
      invokeMock.mockImplementation(async (cmd) => {
        if (cmd === "get_plan_context") {
          throw {
            code: "unexpected_future_error",
            message: "Internal kernel panic in plan resolver backend",
            details: { rawStack: "Error at line 42" },
          };
        }
        return null;
      });

      render(
        <PlanWorkspaceSettings
          config={dummyConfig}
          projectPath="/repo"
          onChange={vi.fn()}
          t={(key) => {
            if (key === "orchestrator.planWorkspace.errors.generic") {
              return "Localized generic error occurred.";
            }
            return String(key);
          }}
        />,
      );

      await waitFor(() => {
        expect(screen.getByText("Localized generic error occurred.")).toBeInTheDocument();
      });

      expect(
        screen.queryByText(/Internal kernel panic in plan resolver backend/),
      ).not.toBeInTheDocument();
      expect(screen.queryByText(/Error at line 42/)).not.toBeInTheDocument();
    });
  });

  describe("formatPlanWorkspaceWarning and cleanup warning UI", () => {
    it("localizes known cleanup warning code without raw English", () => {
      const t = (key: string) => {
        if (key === "orchestrator.planWorkspace.warnings.temporary_link_cleanup_failed") {
          return "Localized: Temporary link could not be removed.";
        }
        return key;
      };

      expect(formatPlanWorkspaceWarning("temporary_link_cleanup_failed", t)).toBe(
        "Localized: Temporary link could not be removed."
      );
    });

    it("falls back to generic localized warning for unknown warning codes", () => {
      const t = (key: string) => {
        if (key === "orchestrator.planWorkspace.warnings.generic") {
          return "Localized generic warning.";
        }
        return key;
      };

      expect(formatPlanWorkspaceWarning("unknown_future_warning", t)).toBe(
        "Localized generic warning."
      );
    });

    it("renders success message and separate localized cleanup warning alert with structured leftover path", async () => {
      invokeMock.mockImplementation(async (cmd) => {
        if (cmd === "get_plan_context") return resolvedContext;
        if (cmd === "preview_new_plan") {
          const preview: PlanNewPreviewResponse = {
            candidateRevision: 23,
            candidateFilename: "V0.24.0-r23.md",
            candidatePath: ".plan/V0.24.0-r23.md",
            token: "preview-token-12345",
            context: resolvedContext,
          };
          return preview;
        }
        if (cmd === "confirm_new_plan") {
          const confirmRes: PlanNewConfirmResponse = {
            createdPlanId: "V0.24.0-r23",
            createdPath: ".plan/V0.24.0-r23.md",
            fileDigest: "newdigest12345",
            context: resolvedContext,
            cleanupWarningCode: "temporary_link_cleanup_failed",
            leftoverTempPath: ".plan/.tmp-new-9999",
          };
          return confirmRes;
        }
        return null;
      });

      render(
        <PlanWorkspaceSettings
          config={dummyConfig}
          projectPath="/repo"
          onChange={vi.fn()}
          t={(key, params) => {
            if (key === "orchestrator.planWorkspace.successCreated") {
              return `Successfully created plan: ${params?.id}`;
            }
            if (key === "orchestrator.planWorkspace.warnings.temporary_link_cleanup_failed") {
              return "Plan was created, but temporary link could not be removed.";
            }
            return String(key);
          }}
        />,
      );

      await waitFor(() => {
        expect(screen.getByText("orchestrator.planWorkspace.resolved")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("orchestrator.planWorkspace.previewNew"));

      await waitFor(() => {
        expect(screen.getByText("V0.24.0-r23.md")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText("orchestrator.planWorkspace.previewModal.confirmBtn"));

      await waitFor(() => {
        expect(screen.getByText("Successfully created plan: V0.24.0-r23")).toBeInTheDocument();
        expect(
          screen.getByText("Plan was created, but temporary link could not be removed."),
        ).toBeInTheDocument();
        expect(screen.getByText(".plan/.tmp-new-9999")).toBeInTheDocument();
      });

      // Assert no raw backend OS text or unformatted diagnostic strings
      expect(screen.queryByText(/temporary_file_cleanup_failed/)).not.toBeInTheDocument();
      expect(screen.queryByText(/PermissionDenied/)).not.toBeInTheDocument();
    });
  });
});
