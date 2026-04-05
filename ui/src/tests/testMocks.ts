import { vi } from "vitest";

export function setupTestMocks() {
  const eventSources: Array<{
    url: string;
    onmessage: ((event: MessageEvent<string>) => void) | null;
    onerror: (() => void) | null;
    close: ReturnType<typeof vi.fn>;
  }> = [];

  Object.defineProperty(window, "localStorage", {
    value: {
      getItem: vi.fn(() => null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
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

  return { eventSources };
}
