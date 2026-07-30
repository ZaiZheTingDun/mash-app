import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

// Radix Checkbox can emit React's generic act() warning from internal provider
// updates under Vitest/React 18. The app behaviour is covered by user-event
// assertions; keeping this warning would make CI logs effectively unreadable.
const originalConsoleError = console.error.bind(console);
function installConsoleErrorFilter() {
  vi.spyOn(console, "error").mockImplementation((...args: unknown[]) => {
    const message = String(args[0] ?? "");
    const stack = args.map((arg) => String(arg)).join("\n");
    if (
      message.includes(
        "When testing, code that causes React state updates should be wrapped into act"
      ) &&
      stack.includes("CheckboxProvider")
    ) {
      return;
    }
    originalConsoleError(...args);
  });
}
installConsoleErrorFilter();

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
  invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "run_startup_migration":
        return { migrated: false, from: null, to: "/tmp/mash-app-data" };
      case "get_servants":
      case "get_craft_essences":
      case "list_projects":
      case "load_battle_scenes":
      case "get_servant_skill_selection":
        return [];
      case "get_active_project_id":
        return null;
      case "get_grand_class_definitions":
        return [];
      case "set_active_project_id":
        return null;
      case "get_app_theme":
        return null;
      case "set_app_theme":
        return null;
      case "pick_asset_bundle":
      case "pick_runtime_bundle":
      case "pick_config_import_file":
        return null;
      case "list_exportable_configs":
        return [];
      case "export_configs":
        return null;
      case "preview_config_import":
        return {
          fileName: "empty.mashconfig.json",
          validConfigs: [],
          invalidItems: [],
        };
      case "import_configurations":
        return { importedProjects: [] };
      case "import_asset_bundle":
        return {
          importedServants: false,
          importedCraftEssences: false,
          servantFiles: 0,
          craftEssenceFiles: 0,
          installDir: "/tmp/mash-assets",
        };
      case "download_asset_bundles":
        return {
          installed: true,
          installedVersion: 2,
          plan: "patch",
          servantFiles: 0,
          craftEssenceFiles: 0,
          installDir: "/tmp/mash-assets",
        };
      case "get_asset_bundle_status":
        return {
          installed: false,
          importedServants: false,
          importedCraftEssences: false,
          servantFiles: 0,
          craftEssenceFiles: 0,
          installDir: "/tmp/mash-assets",
          currentVersion: null,
          appAssetsVersion: 2,
          remoteLatestVersion: null,
          remoteLatestBaseVersion: null,
          targetVersion: 2,
          updateAvailable: true,
          updateDownloadSize: 0,
          updatePlan: "pending",
          latestUrl: "https://mash.xiaotongx.com/mash/assets/latest.json",
          remoteManifestUrl: null,
          updateCheckError: null,
        };
      case "get_self_check_status":
        return {
          appVersion: "0.5.4",
          cvRuntimeVersion: "2026.05.08-runtime1",
          cvRuntimeInstalled: false,
          cvCodeVersion: "2026.05.08-code1",
          cvCodeInstalled: false,
          assetVersion: null,
          appAssetsVersion: 2,
          servants: { entries: 0, hasImage: false, hasJson: false },
          ces: { entries: 0, hasImage: false, hasJson: false },
        };
      case "get_runtime_status":
        return {
          requiredRuntimeVersion: "2026.05.08-runtime1",
          installedRuntimeVersion: null,
          runtimeInstalled: false,
          requiredCodeVersion: "2026.05.08-code1",
          installedCodeVersion: null,
          codeInstalled: false,
          installed: false,
          platform: "darwin-aarch64",
          runtimeDownloadUrl:
            "https://cdn.example.com/mash-cv-runtime-darwin-aarch64-v2026.05.08-runtime1.zip",
          runtimeExpectedSha256:
            "0000000000000000000000000000000000000000000000000000000000000000",
          runtimeInstallDir: "/tmp/runtime/mash-cv/runtime/2026.05.08-runtime1",
          executablePath:
            "/tmp/runtime/mash-cv/runtime/2026.05.08-runtime1/mash-cv-runtime/mash-cv",
          codeDownloadUrl:
            "https://cdn.example.com/mash-cv-code-v2026.05.08-code1.zip",
          codeExpectedSha256:
            "0000000000000000000000000000000000000000000000000000000000000000",
          codeInstallDir: "/tmp/runtime/mash-cv/code/2026.05.08-code1",
          codePath: "/tmp/runtime/mash-cv/code/2026.05.08-code1/mash-cv-code",
        };
      case "import_runtime_bundle":
        return {
          installedKind: "runtime",
          installedVersion: "2026.05.08-runtime1",
          platform: "darwin-aarch64",
          installDir: "/tmp/runtime/mash-cv/runtime/2026.05.08-runtime1",
          executablePath:
            "/tmp/runtime/mash-cv/runtime/2026.05.08-runtime1/mash-cv-runtime/mash-cv",
          codePath: null,
        };
      case "download_runtime_bundles":
        return {
          installed: [
            {
              installedKind: "runtime",
              installedVersion: "2026.05.08-runtime1",
              platform: "darwin-aarch64",
              installDir: "/tmp/runtime/mash-cv/runtime/2026.05.08-runtime1",
              executablePath:
                "/tmp/runtime/mash-cv/runtime/2026.05.08-runtime1/mash-cv-runtime/mash-cv",
              codePath: null,
            },
            {
              installedKind: "code",
              installedVersion: "2026.05.08-code1",
              platform: "darwin-aarch64",
              installDir: "/tmp/runtime/mash-cv/code/2026.05.08-code1",
              executablePath: null,
              codePath: "/tmp/runtime/mash-cv/code/2026.05.08-code1/mash-cv-code",
            },
          ],
        };
      case "check_adb":
        return { connected: false, deviceName: null };
      case "get_selected_adb_device":
        return null;
      case "refresh_adb_devices_with_previews":
        return [];
      case "select_adb_device":
        return { connected: true, deviceName: args?.serial };
      case "connect_adb_port":
        return `127.0.0.1:${args?.port}`;
      case "reset_bluestacks_adb_connection":
        return { ok: true, steps: [] };
      case "get_use_bluestack":
        return false;
      case "get_server":
        return "JP";
      case "get_recognition_settings":
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
          enableExtraClassFilter: true,
        };
      case "set_noble_phantasm_detection_mode":
        return {
          noblePhantasmDetectionMode: args?.value === "gauge" ? "gauge" : "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
          enableExtraClassFilter: true,
        };
      case "set_support_ce_threshold":
      case "set_support_ce_full_gate_threshold":
      case "set_support_mlb_icon_threshold":
      case "set_support_bond_icon_threshold":
      case "set_stop_on_bond_level_up":
      case "set_stop_on_bond_max_level":
      case "set_verify_skill_activation":
      case "set_enable_extra_class_filter":
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp:
            cmd === "set_stop_on_bond_level_up"
              ? Boolean(args?.value)
              : cmd === "set_stop_on_bond_max_level"
                ? false
                : false,
          stopOnBondMaxLevel:
            cmd === "set_stop_on_bond_max_level" ? Boolean(args?.value) : false,
          verifySkillActivation:
            cmd === "set_verify_skill_activation" ? Boolean(args?.value) : false,
          enableExtraClassFilter:
            cmd === "set_enable_extra_class_filter" ? Boolean(args?.value) : true,
        };
      case "should_check_updates_today":
        return false;
      case "mark_update_checked_today":
        return null;
      // Default to "no portrait on disk" so component tests render
      // the placeholder branch unless they explicitly opt in. Tests
      // that want a real `<img>` should `vi.mocked(invoke).mockImpl(...)`.
      case "get_servant_portrait_path":
      case "get_servant_face_path":
      case "get_craft_essence_card_path":
      case "get_template_asset_path":
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

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(async () => null),
}));

// Reset DOM + mock state between tests so one component leaking state
// into the next can't mask a real regression.
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  installConsoleErrorFilter();
});
