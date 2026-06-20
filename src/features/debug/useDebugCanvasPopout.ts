import { useCallback, useEffect, useRef, useState } from "react";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emit, listen } from "../../tauri";
import type { DebugCanvasState } from "./DebugCanvas";
import {
  DEBUG_CANVAS_REQUEST_EVENT,
  DEBUG_CANVAS_STATE_EVENT,
} from "./DebugCanvasWindow";

const DEBUG_CANVAS_POPOUT_WIDTH = 1280;
const DEBUG_CANVAS_POPOUT_HEIGHT = 900;

type DebugLog = (message: string, level?: "info" | "warn" | "error") => void;

/**
 * Keeps the standalone debug canvas window in sync with DebugPage state.
 * The popout renders only the canvas route and asks the host page for a
 * state snapshot over Tauri events when it mounts.
 */
export function useDebugCanvasPopout(
  canvasState: DebugCanvasState,
  log: DebugLog
) {
  const [popoutOpen, setPopoutOpen] = useState(false);
  const canvasStateRef = useRef(canvasState);

  useEffect(() => {
    canvasStateRef.current = canvasState;
    if (popoutOpen) {
      emit(DEBUG_CANVAS_STATE_EVENT, canvasState).catch((err) => {
        console.warn("[DebugPage] emit canvas state failed", err);
      });
    }
  }, [canvasState, popoutOpen]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen(DEBUG_CANVAS_REQUEST_EVENT, () => {
      emit(DEBUG_CANVAS_STATE_EVENT, canvasStateRef.current).catch(() => {});
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => {
        console.warn("[DebugPage] listen canvas request failed", err);
      });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      const win = await WebviewWindow.getByLabel("debug-canvas");
      if (!win) return;
      setPopoutOpen(true);
      unlisten = await win.once("tauri://destroyed", () => {
        setPopoutOpen(false);
      });
    })().catch(() => {});
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const openPopout = useCallback(async () => {
    try {
      const existing = await WebviewWindow.getByLabel("debug-canvas");
      if (existing) {
        await existing.setSize(
          new LogicalSize(DEBUG_CANVAS_POPOUT_WIDTH, DEBUG_CANVAS_POPOUT_HEIGHT)
        );
        await existing.show();
        await existing.setFocus();
        log("已聚焦弹出画面");
        return;
      }
      const win = new WebviewWindow("debug-canvas", {
        url: "index.html#debug-canvas",
        title: "调试画面",
        width: DEBUG_CANVAS_POPOUT_WIDTH,
        height: DEBUG_CANVAS_POPOUT_HEIGHT,
        resizable: true,
      });
      win.once("tauri://created", () => {
        setPopoutOpen(true);
        emit(DEBUG_CANVAS_STATE_EVENT, canvasStateRef.current).catch(
          () => {}
        );
        log("弹出画面已打开");
      });
      win.once("tauri://destroyed", () => {
        setPopoutOpen(false);
        log("弹出画面已关闭");
      });
      win.once("tauri://error", (e) => {
        log(`弹出画面创建失败: ${JSON.stringify(e.payload)}`, "error");
      });
    } catch (err) {
      log(`弹出画面失败: ${err}`, "error");
    }
  }, [log]);

  return { popoutOpen, openPopout };
}
