import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { setupTestMocks } from "./testMocks";

describe("App Navigation", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setupTestMocks();
  });

  it("renders helper copy control in the toolbar, not the sidebar", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue({
        ok: true,
        json: async () => [],
      } as Response);

    const { container } = render(<App />);

    await waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const copyButton = screen.getByRole("button", { name: "Copy" });
    const toolbar = container.querySelector(".toolbar");
    const nav = container.querySelector(".nav");

    expect(toolbar?.contains(copyButton)).toBe(true);
    expect(nav?.contains(copyButton)).toBe(false);
    expect(screen.getByText("cargo run")).toBeInTheDocument();
  });
});
