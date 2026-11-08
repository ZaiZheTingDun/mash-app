import { useCallback, useEffect, useRef, useState } from "react";
import type { CaptureAuthorizationPrompt, CaptureAppList, CaptureClientContext, CaptureHistoryState, CaptureRecord, PacketCaptureWorkspace } from "../../types/packetCapture";
import { listen, type UnlistenFn } from "../../tauri";
import { getPacketCaptureAuthorization, submitPacketCaptureApiKey, getPacketCaptureApps, selectPacketCaptureApp, copyPacketCaptureResponse, deletePacketCaptureRecord, exportPacketCaptureResponse, getPacketCaptureWorkspace, selectPacketCaptureRecord, setupPacketCapture, startPacketCapture, stopPacketCapture } from "./packetCaptureApi";

type Operation = "setup" | "start" | "stop" | "delete" | "copy" | "export";
const pendingMessages: Partial<Record<Operation, string>> = {
  setup: "正在配置环境，请留意设备上的首次启动协议或 root 授权提示…",
  start: "正在启动抓包，请留意设备上的 VPN 授权提示…",
  stop: "正在停止，等待当前设备操作结束…",
  export: "正在导出抓包结果…",
};

export function usePacketCapture() {
  const [authorization, setAuthorization] = useState<CaptureAuthorizationPrompt | null>(null);
  const [clients, setClients] = useState<CaptureAppList | null>(null);
  const [clientOperation, setClientOperation] = useState<"load" | "select" | null>(null);
  const [clientError, setClientError] = useState<string | null>(null);
  const clientInFlight = useRef(false);
  const clientCompletion = useRef<Promise<void> | null>(null);
  const clientGeneration = useRef(0);
  const clientSnapshot = useRef<CaptureAppList | null>(null);
  const clientContextStale = useRef(false);
  const clientRefreshPending = useRef(true);
  const [clientContextVersion, setClientContextVersion] = useState(0);
  const [workspace, setWorkspace] = useState<PacketCaptureWorkspace | null>(null);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [completedOperationGeneration, setCompletedOperationGeneration] = useState(0);
  const [awaitingCaptureStatus, setAwaitingCaptureStatus] = useState(false);
  const [result, setResult] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [exported, setExported] = useState(false);
  const [progressMessage, setProgressMessage] = useState<string | null>(null);
  const progressUnlisten = useRef<UnlistenFn | null>(null);
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
      progressUnlisten.current?.();
      progressUnlisten.current = null;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const subscription = listen("packet-capture-context-changed", () => {
      if (cancelled) return;
      // Invalidate pending discovery too: its reply belongs to the old context.
      clientGeneration.current++;
      clientSnapshot.current = null;
      clientContextStale.current = true;
      clientRefreshPending.current = true;
      setClients(null);
      setClientError(null);
      setResult("");
      setClientContextVersion(version => version + 1);
    });
    void subscription.catch(error => { if (!cancelled) setClientError(String(error)); });
    return () => { cancelled = true; void subscription.then(unlisten => unlisten(), () => {}); };
  }, []);

  useEffect(() => {
    if (operation !== "start") { setAuthorization(null); return; }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const prompt = await getPacketCaptureAuthorization();
        if (!cancelled) setAuthorization(prompt);
      } catch {
        // The start operation remains authoritative; a failed status read is retried.
      } finally {
        if (!cancelled) timer = setTimeout(() => void refresh(), 500);
      }
    };
    void refresh();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [operation]);

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
        setAwaitingCaptureStatus(false);
        setReadError(null);
        if (next.capture.phase === "waiting") timer = setTimeout(() => void refresh(), next.capture.retryAfterMs);
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
  }, [operation, completedOperationGeneration, applyHistory]);

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
    const pendingClientRequest = action === "setup" || action === "start" ? clientCompletion.current : null;
    const generation = ++requestGeneration.current;
    const isCurrent = () => mounted.current && generation === requestGeneration.current;
    resumePollingOnly.current = false;
    inFlight.current = action;
    if (action === "stop") {
      progressUnlisten.current?.();
      progressUnlisten.current = null;
    }
    setOperation(action);
    setError(null);
    setResult("");
    setCopied(false);
    setExported(false);
    setProgressMessage(null);
    try {
      // Device actions wait for an ongoing client query to release its backend
      // device lease. Merely opening the dropdown does not disable the page.
      if (pendingClientRequest) await pendingClientRequest;
      if (!isCurrent()) return;
      await request(isCurrent);
      if (isCurrent() && readError === null && !awaitingCaptureStatus) resumePollingOnly.current = action === "delete" || action === "copy" || action === "export";
    } catch (error) {
      if (isCurrent()) setError(String(error));
    } finally {
      if (generation === requestGeneration.current) {
        inFlight.current = null;
        if (mounted.current) {
          setOperation(null);
          // A fast request can start and finish in one React batch. Still refresh
          // its backend state before releasing the capture navigation lock.
          setCompletedOperationGeneration(generation);
        }
      }
    }
  };
  const updateHistory = (history: CaptureHistoryState) => {
    if (mounted.current) {
      applyHistory(history);
      setWorkspace(previous => previous && { ...previous, history });
    }
  };
  const performWithProgress = (action: "setup" | "start", request: (isCurrent: () => boolean) => Promise<void>) => perform(action, async isCurrent => {
    let active = true;
    // Subscribe before invoke so an immediate device prompt is not missed.
    const unlisten = await listen<string>(`packet-capture-${action}-progress`, event => {
      if (active && isCurrent()) setProgressMessage(event.payload);
    });
    if (!isCurrent()) { active = false; unlisten(); return; }
    progressUnlisten.current = unlisten;
    try {
      await request(isCurrent);
    } finally {
      active = false;
      if (progressUnlisten.current === unlisten) {
        progressUnlisten.current = null;
        unlisten();
      }
      if (isCurrent()) setProgressMessage(null);
    }
  });
  // Client discovery and selection only update the picker. Keeping them outside
  // the capture operation avoids disabling navigation, clearing feedback, or
  // restarting workspace reads whenever the dropdown opens.
  const performClientRequest = useCallback(async (action: "load" | "select", request: () => Promise<CaptureAppList>) => {
    if (clientInFlight.current || inFlight.current || hasCaptureTask || awaitingCaptureStatus) return false;
    const generation = ++clientGeneration.current;
    const isCurrent = () => mounted.current && generation === clientGeneration.current;
    clientInFlight.current = true;
    let complete!: () => void;
    const completion = new Promise<void>(resolve => { complete = resolve; });
    clientCompletion.current = completion;
    setClientOperation(action);
    setClientError(null);
    try {
      const next = await request();
      if (!isCurrent()) return false;
      clientSnapshot.current = next;
      clientContextStale.current = false;
      setClients(next);
      return true;
    } catch (error) {
      if (isCurrent()) setClientError(String(error));
      return false;
    } finally {
      clientInFlight.current = false;
      if (mounted.current) setClientOperation(null);
      if (clientCompletion.current === completion) clientCompletion.current = null;
      complete();
    }
  }, [hasCaptureTask, awaitingCaptureStatus]);

  useEffect(() => {
    // Wait for the old query/operation to release its device lease before reload.
    if (!workspace || !clientRefreshPending.current || operation || clientOperation || hasCaptureTask || awaitingCaptureStatus) return;
    clientRefreshPending.current = false;
    void performClientRequest("load", getPacketCaptureApps);
  }, [workspace, clientContextVersion, operation, clientOperation, hasCaptureTask, awaitingCaptureStatus, performClientRequest]);

  const currentClientContext = (): CaptureClientContext | undefined => {
    if (clientContextStale.current) throw new Error("客户端信息已变更，请刷新客户端列表后重试");
    const list = clientSnapshot.current;
    return list ? { serial: list.serial, server: list.server, selectedPackage: list.selectedPackage } : undefined;
  };
  const captureError = workspace?.capture.phase === "error" ? workspace.capture.message : null;
  const selected = detail.id === selectedId ? detail.record : null;
  const detailError = detail.id === selectedId ? detail.error : null;
  const initialLoading = workspace === null && readError === null;
  const recordLoading = selectedId !== null && selected === null && detailError === null;
  return {
    workspace, authorization: operation === "start" ? authorization : null, submitApiKey: submitPacketCaptureApiKey, clients, clientOperation, clientError, clientBusy: clientOperation !== null, operation, capturing, copied, exported, selectedId, selected, initialLoading, recordLoading, recordError: detailError,
    canStop: hasCaptureTask || awaitingCaptureStatus || operation === "setup" || operation === "start" || operation === "stop",
    navigationLocked: initialLoading || operation !== null || hasCaptureTask || awaitingCaptureStatus,
    busy: operation !== null,
    error: error || (operation === null ? detailError || readError || captureError : null),
    message: ((operation === "setup" || operation === "start") && progressMessage) || (operation && pendingMessages[operation]) || workspace?.capture.message || result,
    loadClients: () => performClientRequest("load", getPacketCaptureApps),
    chooseClient: async (packageName: string) => {
      if (!clients) return false;
      const chosen = await performClientRequest("select", () => selectPacketCaptureApp(clients, packageName));
      if (chosen && mounted.current) {
        setError(null);
        setResult("已选择客户端，请重新进行环境配置");
      }
      return chosen;
    },
    setup: () => performWithProgress("setup", async isCurrent => {
      const response = await setupPacketCapture(currentClientContext());
      if (isCurrent()) setResult(response.message);
    }),
    start: () => performWithProgress("start", async isCurrent => {
      const response = await startPacketCapture(currentClientContext());
      if (isCurrent()) {
        setResult(response.message);
        // Keep navigation locked until the first post-start workspace read.
        setAwaitingCaptureStatus(true);
      }
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
