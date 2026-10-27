import { useEffect, useRef, useState } from "react";
import type { CaptureHistoryState, PacketCaptureWorkspace } from "../../types/packetCapture";
import { listen, type UnlistenFn } from "../../tauri";
import { copyPacketCaptureResponse, deletePacketCaptureRecord, getPacketCaptureWorkspace, selectPacketCaptureRecord, setupPacketCapture, startPacketCapture, stopPacketCapture } from "./packetCaptureApi";

type Operation = "setup" | "start" | "stop" | "select" | "delete" | "copy";
const pendingMessages: Partial<Record<Operation, string>> = {
  setup: "正在配置环境，请留意设备上的首次启动协议或 root 授权提示…",
  start: "正在启动抓包，请留意设备上的 VPN 授权提示…",
  stop: "正在停止，等待当前设备操作结束…",
};

export function usePacketCapture() {
  const [workspace, setWorkspace] = useState<PacketCaptureWorkspace | null>(null);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [result, setResult] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [setupMessage, setSetupMessage] = useState<string | null>(null);
  const setupUnlisten = useRef<UnlistenFn | null>(null);
  const inFlight = useRef<Operation | null>(null);
  const requestGeneration = useRef(0);
  const mounted = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      setupUnlisten.current?.();
      setupUnlisten.current = null;
    };
  }, []);

  useEffect(() => {
    if (operation !== null) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = async () => {
      try {
        const next = await getPacketCaptureWorkspace();
        if (cancelled) return;
        setWorkspace(next);
        setReadError(null);
        if (next.capture.phase === "waiting") timer = setTimeout(() => void refresh(), 1000);
      } catch (error) {
        if (cancelled) return;
        setReadError(String(error));
        // Retry transient reads without abandoning an active capture.
        timer = setTimeout(() => void refresh(), 2000);
      }
    };
    void refresh();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [operation]);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const capturing = workspace?.capture.phase === "waiting";
  const hasCaptureTask = capturing || (workspace?.capture.phase === "error" && workspace.capture.serial !== null);
  const perform = async (action: Operation, request: (isCurrent: () => boolean) => Promise<void>) => {
    if (inFlight.current && !(action === "stop" && inFlight.current !== "stop"
      && (inFlight.current === "setup" || inFlight.current === "start" || hasCaptureTask))) return;
    const generation = ++requestGeneration.current;
    const isCurrent = () => mounted.current && generation === requestGeneration.current;
    inFlight.current = action;
    if (action === "stop") {
      setupUnlisten.current?.();
      setupUnlisten.current = null;
    }
    setOperation(action);
    setError(null);
    setResult("");
    setCopied(false);
    setSetupMessage(null);
    try {
      await request(isCurrent);
    } catch (error) {
      if (isCurrent()) setError(String(error));
    } finally {
      if (generation === requestGeneration.current) {
        inFlight.current = null;
        if (mounted.current) setOperation(null);
      }
    }
  };
  const updateHistory = (history: CaptureHistoryState) => {
    if (mounted.current) setWorkspace(previous => previous && { ...previous, history });
  };
  const captureError = workspace?.capture.phase === "error" ? workspace.capture.message : null;
  return {
    workspace, operation, capturing, copied,
    canStop: hasCaptureTask || operation === "setup" || operation === "start" || operation === "stop",
    busy: operation !== null,
    error: error || (operation === null ? readError || captureError : null),
    message: (operation === "setup" && setupMessage) || (operation && pendingMessages[operation]) || workspace?.capture.message || result,
    setup: () => perform("setup", async isCurrent => {
      let active = true;
      // Subscribe before starting: first-run prompts may arrive before invoke resolves.
      const unlisten = await listen<string>("packet-capture-setup-progress", event => {
        if (active && isCurrent()) setSetupMessage(event.payload);
      });
      if (!isCurrent()) { active = false; unlisten(); return; }
      setupUnlisten.current = unlisten;
      try {
        const response = await setupPacketCapture();
        if (isCurrent()) setResult(response.message);
      } finally {
        active = false;
        if (setupUnlisten.current === unlisten) {
          setupUnlisten.current = null;
          unlisten();
        }
        if (isCurrent()) setSetupMessage(null);
      }
    }),
    start: () => perform("start", async isCurrent => {
      const response = await startPacketCapture();
      if (isCurrent()) setResult(response.message);
    }),
    stop: () => perform("stop", stopPacketCapture),
    select: (id: string) => perform("select", async isCurrent => {
      const history = await selectPacketCaptureRecord(id);
      if (isCurrent()) updateHistory(history);
    }),
    remove: (id: string | null) => perform("delete", async isCurrent => {
      const history = await deletePacketCaptureRecord(id);
      if (isCurrent()) updateHistory(history);
    }),
    copy: () => perform("copy", async isCurrent => {
      await copyPacketCaptureResponse();
      if (isCurrent()) setCopied(true);
    }),
  };
}
