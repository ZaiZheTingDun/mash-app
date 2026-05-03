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
  // jsdom also lacks the Pointer Events API. Radix `<Select>` calls
  // `hasPointerCapture` / `setPointerCapture` / `releasePointerCapture`
  // during open / close transitions and crashes without these stubs.
  // The actual semantics don't matter for our tests — we just need the
  // methods to exist so the click handler doesn't throw.
  type ElementWithCapture = Element & {
    hasPointerCapture?: (id: number) => boolean;
    setPointerCapture?: (id: number) => void;
    releasePointerCapture?: (id: number) => void;
  };
  const proto = Element.prototype as ElementWithCapture;
  if (!proto.hasPointerCapture) {
    proto.hasPointerCapture = () => false;
  }
  if (!proto.setPointerCapture) {
    proto.setPointerCapture = () => {};
  }
  if (!proto.releasePointerCapture) {
    proto.releasePointerCapture = () => {};
  }
}

// Centralized stub for the Tauri IPC bridge. Components that call
// `invoke("get_servants")` etc. should resolve against this mock instead
// of trying to reach a real Tauri runtime (which doesn't exist in
// jsdom). Individual tests can override the mock with `vi.mocked(invoke)
// .mockResolvedValueOnce(...)` for command-specific behaviour.
// `@tauri-apps/api/event` is also unavailable under jsdom. Default the
// `listen` subscription to a no-op so components that hook automation /
// runtime events at mount time (e.g. `StatusBar` listening for
// `automation-status`) don't blow up. Individual tests can override the
// mock to capture the registered handler if they need to simulate
// events.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    switch (cmd) {
      case "get_servants":
      case "get_craft_essences":
      case "list_projects":
      case "load_battle_scenes":
        return [];
      case "check_adb":
        return { connected: false, deviceName: null };
      case "get_use_bluestack":
        return false;
      case "get_server":
        return "JP";
      // Default to "no portrait on disk" so component tests render
      // the placeholder branch unless they explicitly opt in. Tests
      // that want a real `<img>` should `vi.mocked(invoke).mockImpl(...)`.
      case "get_servant_portrait_path":
      case "get_servant_face_path":
      case "get_craft_essence_card_path":
        return null;
      default:
        return null;
    }
  }),
  // `convertFileSrc` normally produces an `asset://localhost/...` URL
  // backed by Tauri's asset protocol. In jsdom we just need a stable
  // string that the component can hand to `<img src>` so assertions
  // can target it.
  convertFileSrc: (path: string) => `asset://${path}`,
}));

// Reset DOM + mock state between tests so one component leaking state
// into the next can't mask a real regression.
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
