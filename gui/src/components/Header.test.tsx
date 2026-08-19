import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import Header from "./Header";

describe("Header Component", () => {
  const baseProps = {
    proxyStatus: "running" as const,
    managedRunning: true,
    proxyLoading: false,
    proxyError: null,
    proxyDiag: null,
    successMessage: null,
    onStart: vi.fn(),
    onStop: vi.fn(),
    onClearDiag: vi.fn(),
    activeTab: "gateway" as const,
  };

  it("renders Gateway controls when activeTab is gateway", () => {
    render(<Header {...baseProps} />);

    expect(screen.getByRole("button", { name: /header\.stopGateway/i })).toBeInTheDocument();
    expect(screen.getByText(/header\.gatewayRunning/i)).toBeInTheDocument();
    expect(screen.queryByText("v0.18.1")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /header\.settings/i })).not.toBeInTheDocument();
  });

  it("returns null unconditionally when activeTab is mcp, even with proxyDiag and proxyError", () => {
    const { container } = render(
      <Header
        {...baseProps}
        activeTab="mcp"
        proxyDiag="Some proxy diagnostic trace"
        proxyError="Failed to connect"
      />
    );

    expect(container.firstChild).toBeNull();
  });

  it("returns null unconditionally when activeTab is settings", () => {
    const { container } = render(
      <Header
        {...baseProps}
        activeTab="settings"
        proxyDiag="Diagnostic info"
      />
    );

    expect(container.firstChild).toBeNull();
  });

  it("does not render version-info or Settings button in Header", () => {
    render(<Header {...baseProps} />);

    expect(screen.queryByText("v0.18.1")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /header\.settings/i })).not.toBeInTheDocument();
  });
});
