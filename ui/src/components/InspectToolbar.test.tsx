import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { InspectToolbar } from "./InspectToolbar";

describe("InspectToolbar", () => {
  it("renders the scoped reindex button as a plain button", () => {
    const onScopedReindex = vi.fn();

    render(
      <InspectToolbar
        apiBase="http://127.0.0.1:8765"
        onApiBaseChange={() => {}}
        onCopyHelper={() => {}}
        onScopedReindex={onScopedReindex}
        projects={[]}
        scopedJob={null}
        selectedProject=""
        selectedTool="codex"
        onSelectProject={() => {}}
        onSelectTool={() => {}}
        scopedReindexDisabled={false}
      />,
    );

    const reindexButton = screen.getByRole("button", { name: "Reindex" });
    expect(reindexButton).toHaveAttribute("type", "button");
    fireEvent.click(reindexButton);
    expect(onScopedReindex).toHaveBeenCalledTimes(1);
  });
});