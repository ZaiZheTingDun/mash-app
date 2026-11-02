import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { openExternalUrl } from "../tauri";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

const url = "https://docs.mash.xiaotongx.com/guides/package-capture/#api-%E6%9C%8D%E5%8A%A1";
const runtimeDescriptor = Object.getOwnPropertyDescriptor(window, "__TAURI_INTERNALS__");

beforeEach(() => {
  vi.mocked(openUrl).mockReset().mockResolvedValue(undefined);
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
  if (runtimeDescriptor) Object.defineProperty(window, "__TAURI_INTERNALS__", runtimeDescriptor);
  else delete window.__TAURI_INTERNALS__;
});

describe("openExternalUrl", () => {
  it("uses the native opener in Tauri even when running the development frontend", async () => {
    vi.stubEnv("DEV", true);
    vi.stubEnv("MODE", "development");
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    const browserOpen = vi.spyOn(window, "open").mockReturnValue(null);
    await openExternalUrl(url);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith(url);
    expect(browserOpen).not.toHaveBeenCalled();
  });

  it("opens a browser tab in the development preview without a Tauri runtime", async () => {
    vi.stubEnv("DEV", true);
    vi.stubEnv("MODE", "development");
    delete window.__TAURI_INTERNALS__;
    const browserOpen = vi.spyOn(window, "open").mockReturnValue(null);
    await openExternalUrl(url);
    expect(browserOpen).toHaveBeenCalledExactlyOnceWith(url, "_blank", "noopener,noreferrer");
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("propagates native failures so the link can show an actionable error", async () => {
    vi.mocked(openUrl).mockRejectedValueOnce(new Error("native open failed"));
    await expect(openExternalUrl(url)).rejects.toThrow("native open failed");
  });
});
