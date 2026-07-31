import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import OpenRouterProviderSection from "./OpenRouterProviderSection";
import type { ProviderConfig, OpenRouterProfile } from "../types";

function makeProfile(
  id: string,
  display_name: string,
  hidden = false,
): OpenRouterProfile {
  return {
    id,
    display_name,
    hidden,
    model_map: {},
    visible_models: [],
    models: {},
  };
}

const provider: ProviderConfig = {
  display_name: "OpenRouter",
  upstream_url: "https://openrouter.ai/api/v1",
  api_key_env: "OPENROUTER_API_KEY",
  default_model: "openrouter/auto",
  force_anthropic_version: null,
  supports_count_tokens: false,
  supports_vision: false,
  supports_video: false,
  supports_thinking: false,
  model_map: {},
  visible_models: [],
  models: {},
  profiles: [],
};

const profiles: OpenRouterProfile[] = [
  makeProfile("uuid-1", "Model 1"),
  makeProfile("uuid-2", "Model 2"),
  makeProfile("uuid-3", "Model 3"),
];

const noop = async () => {};

describe("OpenRouterProfileReordering", () => {
  it("renders one accessible drag handle per profile", async () => {
    render(
      <OpenRouterProviderSection
        providerId="openrouter"
        provider={provider}
        profiles={profiles}
        activeProfileId={null}
        keyStatus={null}
        allKeyStatusLoading={false}
        gatewayRunning={false}
        refreshConfig={noop}
        restartGateway={noop}
        refreshKeyStatus={noop}
        onAddModelSet={noop}
        addError={null}
      />,
    );

    // Expand the accordion by clicking the header row
    const accordionHeader = screen.getByText("OpenRouter");
    await userEvent.click(accordionHeader);

    // One drag handle button per profile, each with the correct aria-label
    expect(
      screen.getAllByRole("button", { name: "Drag to reorder" }),
    ).toHaveLength(profiles.length);
  });
});
