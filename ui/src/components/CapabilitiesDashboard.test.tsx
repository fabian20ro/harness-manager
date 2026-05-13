import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CapabilitiesDashboard } from "./CapabilitiesDashboard";

describe("CapabilitiesDashboard", () => {
  it("shows an actionable empty state when no graph is selected", () => {
    render(<CapabilitiesDashboard graph={null} />);

    expect(
      screen.getByText(
        "Select a project and tool context to see skills, hooks, MCP servers, and instructions.",
      ),
    ).toBeInTheDocument();
  });
});
