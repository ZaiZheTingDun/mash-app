import { useCallback, useEffect, useRef, useState } from "react";
import type { CaptureHistoryState, CaptureRecord, PacketCaptureWorkspace } from "../../types/packetCapture";
import { listen, type UnlistenFn } from "../../tauri";
import { copyPacketCaptureResponse, deletePacketCaptureRecord, exportPacketCaptureResponse, getPacketCaptureWorkspace, selectPacketCaptureRecord, setupPacketCapture, startPacketCapture, stopPacketCapture } from "./packetCaptureApi";

type Operation = "setup" | "start" | "stop" | "delete" | "copy" | "export";
const pendingMessages: Partial<Record<Operation, string>> = {
  setup: "正在配置环境，请留意设备上的首次启动协议或 root 授权提示…",
  start: "正在启动抓包，请留意设备上的 VPN 授权提示…",
  stop: "正在停止，等待当前设备操作结束…",
  export: "正在导出抓包结果…",
};

export function usePacketCapture() {
  const [workspace, setWorkspace] = useState<PacketCaptureWorkspace | null>(null);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [result, setResult] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [exported, setExported] = useState(false);
  const [setupMessage, setSetupMessage] = useState<string | null>(null);
  const setupUnlisten = useRef<UnlistenFn | null>(null);
  const inFlight = useRef<Operation | null>(null);
  const requestGeneration = useRef(0);
  const mounted = useRef(false);
  const resumePollingOnly = useRef(false);
  const waitingForCapture = useRef(false);
  const historyIndex = useRef<CaptureHistoryState | null>(null);
  const desiredRecordId = useRef<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detailRetry, setDetailRetry] = useState(0);
  const [detail, setDetail] = useState<{ id: string | null; record: CaptureRecord | null; error: string | null }>({ id: null, record: null, error: null });

  const applyHistory = useCallback((history: CaptureHistoryState) => {
    const previous = historyIndex.current;
    const latest = history.records[0]?.id;
    const newCapture = latest && previous && !previous.records.some(entry => entry.id === latest);
    if (!previous || newCapture || !history.records.some(entry => entry.id === desiredRecordId.current)) {
      desiredRecordId.current = history.defaultRecordId;
      setSelectedId(history.defaultRecordId);
      setCopied(false);
      setExported(false);
    }
    historyIndex.current = history;
  }, []);

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
        waitingForCapture.current = next.capture.phase === "waiting";
        applyHistory(next.history);
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
    // Deletion returns the updated index; copy/export do not change it. Avoid an
    // immediate duplicate request, but resume an active capture's polling.
    if (resumePollingOnly.current) {
      resumePollingOnly.current = false;
      if (waitingForCapture.current) timer = setTimeout(() => void refresh(), 1000);
    } else {
      void refresh();
    }
    return () => { cancelled = true; clearTimeout(timer); };
  }, [operation, applyHistory]);

  // Commit selection before starting the slow read. Keep one displayed record;
  // bounded caching belongs to Rust. Late results never change the selection.
  useEffect(() => {
    let cancelled = false;
    setDetail({ id: selectedId, record: null, error: null });
    if (selectedId !== null) {
      void selectPacketCaptureRecord(selectedId).then(record => {
        if (cancelled || desiredRecordId.current !== selectedId) return;
        if (record.entry.id !== selectedId) throw new Error("抓包记录标识不一致");
        setDetail({ id: selectedId, record, error: null });
      }).catch(error => {
        if (!cancelled && desiredRecordId.current === selectedId) setDetail({ id: selectedId, record: null, error: String(error) });
      });
    }
    return () => { cancelled = true; };
  }, [selectedId, detailRetry]);

  useEffect(() => {
    if (!copied && !exported) return;
    const timer = setTimeout(() => { setCopied(false); setExported(false); }, 2000);
    return () => clearTimeout(timer);
  }, [copied, exported]);

  const capturing = workspace?.capture.phase === "waiting";
  const hasCaptureTask = capturing || (workspace?.capture.phase === "error" && workspace.capture.serial !== null);
  const perform = async (action: Operation, request: (isCurrent: () => boolean) => Promise<void>) => {
    if (inFlight.current && !(action === "stop" && inFlight.current !== "stop"
      && (inFlight.current === "setup" || inFlight.current === "start" || hasCaptureTask))) return;
    const generation = ++requestGeneration.current;
    const isCurrent = () => mounted.current && generation === requestGeneration.current;
    resumePollingOnly.current = false;
    inFlight.current = action;
    if (action === "stop") {
      setupUnlisten.current?.();
      setupUnlisten.current = null;
    }
    setOperation(action);
    setError(null);
    setResult("");
    setCopied(false);
    setExported(false);
    setSetupMessage(null);
    try {
      await request(isCurrent);
      if (isCurrent() && readError === null) resumePollingOnly.current = action === "delete" || action === "copy" || action === "export";
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
    if (mounted.current) {
      applyHistory(history);
      setWorkspace(previous => previous && { ...previous, history });
    }
  };
  const captureError = workspace?.capture.phase === "error" ? workspace.capture.message : null;
  const selected = detail.id === selectedId ? detail.record : null;
  const detailError = detail.id === selectedId ? detail.error : null;
  const initialLoading = workspace === null && readError === null;
  const recordLoading = selectedId !== null && selected === null && detailError === null;
  return {
    workspace, operation, capturing, copied, exported, selectedId, selected, initialLoading, recordLoading, recordError: detailError,
    canStop: hasCaptureTask || operation === "setup" || operation === "start" || operation === "stop",
    busy: operation !== null,
    error: error || (operation === null ? detailError || readError || captureError : null),
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
    select: (id: string) => {
      if (inFlight.current || !historyIndex.current?.records.some(entry => entry.id === id)) return;
      desiredRecordId.current = id;
      setSelectedId(id);
      setCopied(false);
      setExported(false);
      setError(null);
      if (id === selectedId && detailError) setDetailRetry(value => value + 1);
    },
    remove: (id: string | null) => perform("delete", async isCurrent => {
      const history = await deletePacketCaptureRecord(id);
      if (isCurrent()) updateHistory(history);
    }),
    copy: () => perform("copy", async isCurrent => {
      if (!selected || recordLoading) return;
      await copyPacketCaptureResponse(selected.entry.id);
      if (isCurrent()) setCopied(true);
    }),
    exportResponse: () => perform("export", async isCurrent => {
      if (!selected || recordLoading) return;
      const path = await exportPacketCaptureResponse(selected.entry.id);
      if (isCurrent() && path !== null) setExported(true);
    }),
  };
}
