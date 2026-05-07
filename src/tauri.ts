import {
  convertFileSrc as tauriConvertFileSrc,
  invoke as tauriInvoke,
} from "@tauri-apps/api/core";
import {
  emit as tauriEmit,
  listen as tauriListen,
  type Event,
  type UnlistenFn,
} from "@tauri-apps/api/event";
import { invokeDevMock } from "./devMock";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

type InvokeArgs = Record<string, unknown>;

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;
}

function shouldUseDevMock(): boolean {
  return import.meta.env.DEV && import.meta.env.MODE !== "test" && !hasTauriRuntime();
}

export function invoke<T = unknown>(cmd: string, args?: InvokeArgs): Promise<T> {
  if (shouldUseDevMock()) {
    return invokeDevMock<T>(cmd, args);
  }
  if (args === undefined) {
    return tauriInvoke<T>(cmd);
  }
  return tauriInvoke<T>(cmd, args);
}

export function convertFileSrc(filePath: string, protocol?: string): string {
  if (shouldUseDevMock()) {
    return filePath;
  }
  return tauriConvertFileSrc(filePath, protocol);
}

export function listen<T>(
  event: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  if (shouldUseDevMock()) {
    void handler;
    return Promise.resolve(() => {});
  }
  return tauriListen<T>(event, handler);
}

export function emit<T>(event: string, payload?: T): Promise<void> {
  if (shouldUseDevMock()) {
    void event;
    void payload;
    return Promise.resolve();
  }
  return tauriEmit(event, payload);
}

export type { Event, UnlistenFn };
