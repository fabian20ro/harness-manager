import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

describe("App", () => {
  let eventSources: Array<{
    url: string;
    onmessage: ((event: MessageEvent<string>) => void) | null;
    onerror: (() => void) | null;
    close: ReturnType<typeof vi.fn>;
  }>;

  beforeEach(() => {
    vi.restoreAllMocks();
    eventSources = [];
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn(() => null),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });
    Object.defineProperty(window, "EventSource", {
      value: class MockEventSource {
        url: string;
        onmessage: ((event: MessageEvent<string>) => void) | null = null;
        onerror: (() => void) | null = null;
        close = vi.fn();

        constructor(url: string) {
          this.url = url;
          eventSources.push(this);
        }
      },
      configurable: true,
    });
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

  it("renders bottom scan status bar from event stream updates", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      json: async () => [],
    } as Response);

    render(<App />);

    await waitFor(() => expect(eventSources).toHaveLength(1));

    await act(async () => {
      eventSources[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            id: "job-1",
            kind: "scan",
            status: "running",
            created_at: "2026-03-29T10:00:00Z",
            finished_at: null,
            message: "Scanning ~/git/demo",
            current_path: "~/git/demo",
            items_done: 1,
            items_total: 2,
          }),
        }),
      );
    });

    expect(screen.getByRole("status", { name: "Scan status" })).toBeInTheDocument();
    expect(screen.getByText("Scanning ~/git/demo")).toBeInTheDocument();
    vi.useFakeTimers();

    await act(async () => {
      eventSources[0].onmessage?.(
        new MessageEvent("message", {
          data: JSON.stringify({
            id: "job-1",
            kind: "scan",
            status: "completed",
            created_at: "2026-03-29T10:00:00Z",
            finished_at: "2026-03-29T10:00:03Z",
            message: "Indexed 1 project(s).",
          }),
        }),
      );
    });

    expect(screen.getByText("Indexed 1 project(s).")).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(4_001);
    });
    expect(screen.queryByRole("status", { name: "Scan status" })).not.toBeInTheDocument();
    vi.useRealTimers();
  });
});
