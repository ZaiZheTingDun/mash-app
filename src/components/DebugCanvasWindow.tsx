import { useEffect, useState } from "react";
import { Flex, Text } from "@radix-ui/themes";
import { listen, emit } from "../tauri";
import { DebugCanvas, type DebugCanvasState } from "./DebugCanvas";

const EMPTY_STATE: DebugCanvasState = {
  imageSrc: null,
  probes: [],
  commandCards: [],
  noblePhantasms: [],
  battleScene: null,
  attackButton: null,
  enhancementServantResult: null,
  supportResult: null,
  coordinates: null,
  showCoordOverlay: false,
  visibleCoordGroups: [],
};

/** Tauri event names shared with `DebugPage`. Keep them in lock step. */
export const DEBUG_CANVAS_STATE_EVENT = "debug-canvas:state";
export const DEBUG_CANVAS_REQUEST_EVENT = "debug-canvas:request";

/**
 * Standalone view rendered in the popout `WebviewWindow` so the user
 * can stretch the screenshot to a full secondary monitor without the
 * toolbars / side panel competing for space. Owns no debug state of its
 * own — listens on `debug-canvas:state` for snapshots emitted by the
 * main `DebugPage` and re-renders. On mount it broadcasts
 * `debug-canvas:request` so the host can re-emit the latest state and
 * the popout doesn't wait for the next interaction to populate.
 */
export function DebugCanvasWindow() {
  const [state, setState] = useState<DebugCanvasState>(EMPTY_STATE);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      unlisten = await listen<DebugCanvasState>(
        DEBUG_CANVAS_STATE_EVENT,
        (event) => {
          setState(event.payload);
        }
      );
      // Ask the main window to broadcast its current snapshot. The host
      // listens for this and re-emits state, so we don't render an
      // empty canvas until the next probe runs.
      await emit(DEBUG_CANVAS_REQUEST_EVENT, null);
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  return (
    <Flex
      direction="column"
      style={{
        width: "100vw",
        height: "100vh",
        background: "var(--gray-2)",
        padding: 8,
        boxSizing: "border-box",
      }}
    >
      <Flex
        justify="between"
        align="center"
        style={{ padding: "0 4px 6px", flexShrink: 0 }}
      >
        <Text size="2" weight="medium">
          调试画面 (popout)
        </Text>
        <Text size="1" color="gray">
          实时跟随主窗口 · 点击主窗口的按钮即可刷新
        </Text>
      </Flex>
      <DebugCanvas
        {...state}
        className="debug-canvas-popout"
        placeholder="等待主窗口截取画面…"
      />
    </Flex>
  );
}
