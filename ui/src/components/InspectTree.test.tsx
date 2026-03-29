import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { InspectTree } from "./InspectTree";

describe("InspectTree", () => {
  it("renders usage indicators for tree nodes", () => {
    render(
      <InspectTree
        selectedNodeId="node-1"
        onSelect={() => {}}
        tree={[
          {
            key: "node-1",
            label: "AGENTS.md",
            nodeId: "node-1",
            path: "~/git/demo/AGENTS.md",
            children: [],
            states: ["effective"],
            usageState: "used",
            score: 0,
          },
          {
            key: "node-2",
            label: "missing.md",
            nodeId: "node-2",
            path: "~/git/demo/missing.md",
            children: [],
            states: ["broken_reference"],
            usageState: "broken",
            score: 3,
          },
        ]}
      />,
    );

    expect(screen.getByRole("button", { name: /AGENTS.md/ })).toHaveClass("usage-used");
    expect(screen.getByRole("button", { name: /missing.md/ })).toHaveClass("usage-broken");
  });
});
