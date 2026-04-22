import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

// jsdom doesn't implement matchMedia or ResizeObserver, both of which
// Radix UI Themes touches during dialog rendering. Stub them with
// no-op implementations so component tests can mount Radix components
// without crashing.
if (typeof window !== "undefined") {
  if (!window.matchMedia) {
    window.matchMedia = (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    });
  }
  if (!window.ResizeObserver) {
    window.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
  // jsdom doesn't implement `Element.scrollIntoView` — a few of our
  // dialog components call it during keyboard navigation. A no-op
  // satisfies the call without affecting test assertions.
  if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = function () {};
  }
}

// Centralized stub for the Tauri IPC bridge. Components that call
// `invoke("get_servants")` etc. should resolve against this mock instead
// of trying to reach a real Tauri runtime (which doesn't exist in
// jsdom). Individual tests can override the mock with `vi.mocked(invoke)
// .mockResolvedValueOnce(...)` for command-specific behaviour.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    switch (cmd) {
      case "get_servants":
      case "get_craft_essences":
      case "list_projects":
      case "load_turns":
        return [];
      case "check_adb":
        return { ready: false, devices: [] };
      case "get_use_bluestack":
        return false;
      default:
        return null;
    }
  }),
}));

// Reset DOM + mock state between tests so one component leaking state
// into the next can't mask a real regression.
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
