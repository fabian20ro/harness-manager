import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ViewerPane } from "./ViewerPane";

describe("ViewerPane", () => {
  it("renders wrapped viewer content in a dedicated pre element", () => {
    render(<ViewerPane nodeKey="node-1" content={"alpha ".repeat(50)} />);
    const viewer = screen.getByText(/alpha alpha/);
    expect(viewer.tagName).toBe("PRE");
    expect(viewer).toHaveClass("viewer-pre");
  });

  it("shows edit controls for editable nodes", () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <ViewerPane
        nodeKey="node-1"
        content="alpha"
        editable
        versionToken="v1"
        lastSavedBackupAvailable
        onSave={onSave}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "beta" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledWith("beta", "v1");
  });
});
