import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { ChevronLeftIcon, ReloadIcon } from "@radix-ui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type { CvConfig } from "../types/cv";

interface DebugScreenSize {
  w: number;
  h: number;
}

interface DebugCaptureResult {
  imagePath: string;
  screen: string;
  score: number;
  screenSize: DebugScreenSize | null;
}

interface NormRectDto {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface ElementMatchDto {
  found: boolean;
  x: number;
  y: number;
  score: number;
  region: NormRectDto | null;
}

interface ProbeResult {
  label: string;
  threshold: number;
  match: ElementMatchDto;
  timestamp: string;
}

interface LogEntry {
  time: string;
  level: "info" | "warn" | "error";
  message: string;
}

interface DebugPageProps {
  onBack: () => void;
}

function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

export function DebugPage({ onBack }: DebugPageProps) {
  const [capture, setCapture] = useState<DebugCaptureResult | null>(null);
  const [cacheBuster, setCacheBuster] = useState(0);
  const [imgNaturalSize, setImgNaturalSize] = useState<{
    w: number;
    h: number;
  } | null>(null);

  const [cvConfig, setCvConfig] = useState<CvConfig | null>(null);
  const [templateKeys, setTemplateKeys] = useState<string[]>([]);
  const [selectedScreen, setSelectedScreen] = useState<string>("");
  const [selectedElement, setSelectedElement] = useState<string>("");
  const [rawTemplateInput, setRawTemplateInput] = useState("");
  const [threshold, setThreshold] = useState(0.8);

  const [probes, setProbes] = useState<ProbeResult[]>([]);
  const [capturing, setCapturing] = useState(false);
  const [probing, setProbing] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);
  const didShutdown = useRef(false);

  const log = useCallback(
    (message: string, level: LogEntry["level"] = "info") => {
      setLogs((prev) => [...prev, { time: timestamp(), level, message }]);
      const tag = `[DebugPage ${level}]`;
      if (level === "error") console.error(tag, message);
      else if (level === "warn") console.warn(tag, message);
      else console.log(tag, message);
    },
    []
  );

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await invoke<CvConfig>("debug_get_cv_config");
      setCvConfig(cfg);
      const screenNames = Object.keys(cfg.screens || {});
      log(`已加载 cv.json，screens: ${screenNames.join(", ") || "(空)"}`);
      if (screenNames.length > 0 && !selectedScreen) {
        setSelectedScreen(screenNames[0]);
      }
    } catch (err) {
      log(`读取 cv.json 失败: ${err}`, "error");
    }
  }, [log, selectedScreen]);

  const loadTemplateList = useCallback(async () => {
    try {
      const keys = await invoke<string[]>("debug_list_templates");
      setTemplateKeys(keys);
      log(
        `已加载 ${keys.length} 个模板${keys.length > 0 ? ": " + keys.slice(0, 8).join(", ") + (keys.length > 8 ? " …" : "") : ""}`
      );
    } catch (err) {
      log(`读取模板列表失败: ${err}`, "error");
    }
  }, [log]);

  useEffect(() => {
    log("初始化 CV 调试页面");
    loadConfig();
    loadTemplateList();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    return () => {
      if (didShutdown.current) return;
      didShutdown.current = true;
      invoke("debug_shutdown").catch(() => {});
    };
  }, []);

  // Auto-select the first element when screen changes
  const elementNames = useMemo(() => {
    if (!cvConfig || !selectedScreen) return [];
    const screen = cvConfig.screens[selectedScreen];
    return screen?.elements ? Object.keys(screen.elements) : [];
  }, [cvConfig, selectedScreen]);

  useEffect(() => {
    if (elementNames.length > 0 && !elementNames.includes(selectedElement)) {
      setSelectedElement(elementNames[0]);
    } else if (elementNames.length === 0) {
      setSelectedElement("");
    }
  }, [elementNames, selectedElement]);

  const screenNames = useMemo(
    () => (cvConfig ? Object.keys(cvConfig.screens) : []),
    [cvConfig]
  );

  const selectedElementSpec = useMemo(() => {
    if (!cvConfig || !selectedScreen || !selectedElement) return null;
    return cvConfig.screens[selectedScreen]?.elements?.[selectedElement] ?? null;
  }, [cvConfig, selectedScreen, selectedElement]);

  const handleCapture = useCallback(async () => {
    setCapturing(true);
    log("调用 debug_capture…");
    try {
      const result = await invoke<DebugCaptureResult>("debug_capture");
      setCapture(result);
      setCacheBuster(Date.now());
      setProbes([]);
      setImgNaturalSize(null);
      log(
        `截图成功 | 画面 = ${result.screen} (score=${result.score.toFixed(3)})` +
          (result.screenSize
            ? ` | 设备 = ${result.screenSize.w}×${result.screenSize.h}`
            : "")
      );
      if (result.screen !== "Unknown" && screenNames.includes(result.screen)) {
        setSelectedScreen(result.screen);
      }
    } catch (err) {
      log(`debug_capture 失败: ${err}`, "error");
    } finally {
      setCapturing(false);
    }
  }, [log, screenNames]);

  const handleProbeByName = useCallback(async () => {
    if (!selectedScreen || !selectedElement) return;
    setProbing(true);
    log(
      `调用 debug_find_element_by_name (screen=${selectedScreen}, element=${selectedElement})`
    );
    try {
      const match = await invoke<ElementMatchDto>(
        "debug_find_element_by_name",
        { screen: selectedScreen, element: selectedElement }
      );
      const label = `${selectedScreen}.${selectedElement}`;
      const effectiveThreshold = selectedElementSpec?.threshold ?? 0.8;
      setProbes((prev) => [
        {
          label,
          threshold: effectiveThreshold,
          match,
          timestamp: timestamp(),
        },
        ...prev,
      ]);
      if (match.found) {
        log(
          `命中: ${label} | score=${match.score.toFixed(3)} | 中心=(${match.x.toFixed(3)}, ${match.y.toFixed(3)})`
        );
      } else {
        log(
          `未命中: ${label} | score=${match.score.toFixed(3)} (阈值=${effectiveThreshold})`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_find_element_by_name 失败: ${err}`, "error");
    } finally {
      setProbing(false);
    }
  }, [selectedScreen, selectedElement, selectedElementSpec, log]);

  const handleProbeRaw = useCallback(async () => {
    const key = rawTemplateInput.trim();
    if (!key) return;
    setProbing(true);
    log(`调用 debug_find_element (templateKey=${key}, threshold=${threshold})`);
    try {
      const match = await invoke<ElementMatchDto>("debug_find_element", {
        templateKey: key,
        threshold,
      });
      setProbes((prev) => [
        { label: key, threshold, match, timestamp: timestamp() },
        ...prev,
      ]);
      if (match.found) {
        log(`命中: ${key} | score=${match.score.toFixed(3)}`);
      } else {
        log(
          `未命中: ${key} | score=${match.score.toFixed(3)} (阈值=${threshold})`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_find_element 失败: ${err}`, "error");
    } finally {
      setProbing(false);
    }
  }, [rawTemplateInput, threshold, log]);

  const handleClearOverlays = useCallback(() => {
    setProbes([]);
    log("已清除标注");
  }, [log]);

  const handleClearLogs = useCallback(() => setLogs([]), []);

  const handleReloadSidecar = useCallback(async () => {
    setReloading(true);
    log("重新加载 sidecar…");
    try {
      await invoke("debug_reload_sidecar");
      log("sidecar 已重启，模板与 cv.json 已重新加载");
      await loadConfig();
      await loadTemplateList();
    } catch (err) {
      log(`重新加载失败: ${err}`, "error");
    } finally {
      setReloading(false);
    }
  }, [log, loadConfig, loadTemplateList]);

  const handleImgLoad = useCallback(
    (e: React.SyntheticEvent<HTMLImageElement>) => {
      const img = e.currentTarget;
      setImgNaturalSize({ w: img.naturalWidth, h: img.naturalHeight });
      log(`图片加载成功 (${img.naturalWidth}×${img.naturalHeight})`);
    },
    [log]
  );

  const handleImgError = useCallback(() => {
    log(
      "图片加载失败 — 可能是 assetProtocol 未启用或路径不在 scope 内。",
      "error"
    );
  }, [log]);

  const imageSrc = capture
    ? `${convertFileSrc(capture.imagePath)}?t=${cacheBuster}`
    : null;

  return (
    <Flex direction="column" className="debug-page">
      <Flex
        align="center"
        justify="between"
        gap="3"
        className="debug-header"
      >
        <Flex align="center" gap="3">
          <button className="battle-back-btn" onClick={onBack}>
            <ChevronLeftIcon width={16} height={16} />
            <Text size="2">返回</Text>
          </button>
          <Text size="4" weight="bold">
            CV 调试
          </Text>
        </Flex>
        <button
          className="debug-reload-btn"
          disabled={reloading}
          onClick={handleReloadSidecar}
          title="重新加载 sidecar, cv.json 和模板"
        >
          <ReloadIcon width={14} height={14} />
          <Text size="1">{reloading ? "重载中…" : "重载模板/配置"}</Text>
        </button>
      </Flex>

      <Flex className="debug-body" gap="4">
        <Flex direction="column" className="debug-canvas-col" gap="3">
          <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
            <button
              className="battle-btn battle-btn-start"
              disabled={capturing}
              onClick={handleCapture}
            >
              {capturing ? "截取中…" : "截取画面"}
            </button>

            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                画面
              </Text>
              <select
                className="debug-select"
                value={selectedScreen}
                onChange={(e) => setSelectedScreen(e.target.value)}
                disabled={screenNames.length === 0}
              >
                {screenNames.length === 0 && <option value="">—</option>}
                {screenNames.map((s) => (
                  <option key={s} value={s}>
                    {s}
                  </option>
                ))}
              </select>
            </Flex>

            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                元素
              </Text>
              <select
                className="debug-select"
                value={selectedElement}
                onChange={(e) => setSelectedElement(e.target.value)}
                disabled={elementNames.length === 0}
              >
                {elementNames.length === 0 && <option value="">—</option>}
                {elementNames.map((e) => (
                  <option key={e} value={e}>
                    {e}
                  </option>
                ))}
              </select>
            </Flex>

            <button
              className="battle-btn battle-btn-start"
              disabled={
                probing || !capture || !selectedScreen || !selectedElement
              }
              onClick={handleProbeByName}
            >
              {probing ? "查找中…" : "查找元素"}
            </button>

            <button
              className="battle-btn battle-btn-stop"
              disabled={probes.length === 0}
              onClick={handleClearOverlays}
            >
              清除标注
            </button>
          </Flex>

          <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
            <Text size="1" color="gray">
              原始模板探针
            </Text>
            <input
              className="debug-input"
              list="debug-template-list"
              placeholder="模板 key"
              value={rawTemplateInput}
              onChange={(e) => setRawTemplateInput(e.target.value)}
            />
            <datalist id="debug-template-list">
              {templateKeys.map((k) => (
                <option key={k} value={k} />
              ))}
            </datalist>
            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                阈值
              </Text>
              <input
                className="debug-input debug-input-threshold"
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={threshold}
                onChange={(e) =>
                  setThreshold(
                    Math.max(0, Math.min(1, Number(e.target.value) || 0))
                  )
                }
              />
            </Flex>
            <button
              className="battle-btn battle-btn-start debug-btn-small"
              disabled={probing || !capture || !rawTemplateInput.trim()}
              onClick={handleProbeRaw}
            >
              {probing ? "查找中…" : "查找"}
            </button>
          </Flex>

          <Box className="debug-canvas-wrapper">
            {imageSrc ? (
              <Box className="debug-canvas">
                <img
                  src={imageSrc}
                  alt="screenshot"
                  className="debug-canvas-img"
                  onLoad={handleImgLoad}
                  onError={handleImgError}
                />
                {probes.map((p, idx) =>
                  p.match.found && p.match.region ? (
                    <Box
                      key={`box-${idx}`}
                      className="debug-overlay-box"
                      style={{
                        left: `${p.match.region.x * 100}%`,
                        top: `${p.match.region.y * 100}%`,
                        width: `${p.match.region.w * 100}%`,
                        height: `${p.match.region.h * 100}%`,
                      }}
                    >
                      <span className="debug-overlay-label">
                        {p.label} · {p.match.score.toFixed(2)}
                      </span>
                    </Box>
                  ) : null
                )}
              </Box>
            ) : (
              <Flex
                align="center"
                justify="center"
                className="debug-canvas-placeholder"
              >
                <Text size="2" color="gray">
                  点击 截取画面 开始
                </Text>
              </Flex>
            )}
          </Box>

          <Box className="debug-log-container">
            <Flex justify="between" align="center" style={{ marginBottom: 6 }}>
              <Text size="1" color="gray" className="debug-side-label">
                调试日志
              </Text>
              <button
                className="debug-log-clear"
                disabled={logs.length === 0}
                onClick={handleClearLogs}
              >
                清空
              </button>
            </Flex>
            <Box className="debug-log">
              {logs.length === 0 && (
                <Text size="1" color="gray">
                  暂无日志
                </Text>
              )}
              {logs.map((entry, i) => (
                <div
                  key={i}
                  className={`debug-log-entry debug-log-${entry.level}`}
                >
                  <span className="debug-log-time">{entry.time}</span>
                  <span className="debug-log-msg">{entry.message}</span>
                </div>
              ))}
              <div ref={logEndRef} />
            </Box>
          </Box>
        </Flex>

        <Flex direction="column" className="debug-side" gap="3">
          <Box>
            <Text size="1" color="gray" className="debug-side-label">
              识别画面
            </Text>
            <Box className="debug-screen-badge">
              <Text size="3" weight="bold">
                {capture?.screen ?? "—"}
              </Text>
              {capture && (
                <Text size="1" color="gray">
                  score {capture.score.toFixed(3)}
                </Text>
              )}
              {capture?.screenSize && (
                <Text size="1" color="gray">
                  设备 {capture.screenSize.w} × {capture.screenSize.h}
                </Text>
              )}
              {imgNaturalSize && (
                <Text size="1" color="gray">
                  图片 {imgNaturalSize.w} × {imgNaturalSize.h}
                </Text>
              )}
            </Box>
          </Box>

          {selectedElementSpec && (
            <Box>
              <Text size="1" color="gray" className="debug-side-label">
                当前元素配置
              </Text>
              <Box className="debug-screen-badge">
                <Text size="2" weight="medium">
                  {selectedScreen}.{selectedElement}
                </Text>
                <Text size="1" color="gray">
                  模板: {selectedElementSpec.template}
                </Text>
                {selectedElementSpec.region && (
                  <Text size="1" color="gray">
                    区域:{" "}
                    {`(${selectedElementSpec.region.x.toFixed(2)}, ${selectedElementSpec.region.y.toFixed(2)}, ${selectedElementSpec.region.w.toFixed(2)}, ${selectedElementSpec.region.h.toFixed(2)})`}
                  </Text>
                )}
                {selectedElementSpec.threshold !== undefined && (
                  <Text size="1" color="gray">
                    阈值: {selectedElementSpec.threshold}
                  </Text>
                )}
              </Box>
            </Box>
          )}

          <Box>
            <Text size="1" color="gray" className="debug-side-label">
              匹配历史
            </Text>
            <Box className="debug-match-list">
              {probes.length === 0 && (
                <Text size="1" color="gray">
                  暂无查询
                </Text>
              )}
              {probes.map((p, idx) => (
                <Box
                  key={`probe-${idx}`}
                  className={`debug-match-entry ${p.match.found ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      {p.label}
                    </Text>
                    <Text size="1" color="gray">
                      {p.timestamp}
                    </Text>
                  </Flex>
                  <Text size="1" color={p.match.found ? "green" : "red"}>
                    {p.match.found ? "命中" : "未命中"} · score{" "}
                    {p.match.score.toFixed(3)} · 阈值 {p.threshold.toFixed(2)}
                  </Text>
                  {p.match.found && (
                    <Text size="1" color="gray">
                      中心 ({p.match.x.toFixed(3)}, {p.match.y.toFixed(3)})
                    </Text>
                  )}
                </Box>
              ))}
            </Box>
          </Box>
        </Flex>
      </Flex>
    </Flex>
  );
}
