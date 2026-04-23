import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  ReloadIcon,
} from "@radix-ui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type { CvConfig } from "../types/cv";

/**
 * Collapsible group used throughout the debug UI. Built on the native
 * `<details>` element so it needs zero React state and is naturally
 * keyboard-accessible. The chevron rotates via a CSS rule on
 * `.debug-section[open] > summary > .debug-section-chevron`.
 */
function DebugSection({
  title,
  defaultOpen = false,
  badge,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  badge?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <details className="debug-section" open={defaultOpen}>
      <summary className="debug-section-summary">
        <ChevronRightIcon
          className="debug-section-chevron"
          width={14}
          height={14}
        />
        <Text size="2" weight="medium">
          {title}
        </Text>
        {badge !== undefined && badge !== null && (
          <Text size="1" color="gray" className="debug-section-badge">
            {badge}
          </Text>
        )}
      </summary>
      <div className="debug-section-body">{children}</div>
    </details>
  );
}

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

interface PointDto {
  x: number;
  y: number;
}

interface LabeledPointDto {
  label: string;
  point: PointDto;
}

interface LabeledRegionDto {
  label: string;
  region: NormRectDto;
}

interface CoordGroupDto {
  id: string;
  label: string;
  points: LabeledPointDto[];
  regions: LabeledRegionDto[];
}

interface RunnerCoordinatesDto {
  groups: CoordGroupDto[];
}

interface CommandCardMatchDto {
  slot: number;
  x: number;
  y: number;
  cardRegion: NormRectDto;
  faceRegion: NormRectDto;
  suit?: "a" | "b" | "q";
  iconScore?: number;
  iconRegion?: NormRectDto;
  servantId?: number;
  ascension?: number;
  faceScore?: number;
}

interface NoblePhantasmMatchDto {
  slot: number;
  cardRegion: NormRectDto;
  ready: boolean;
  edgeFrac: number;
  stdBgr: number;
}

interface DigitMatchDto {
  value: number;
  score: number;
  region: NormRectDto;
}

interface BattleSceneResultDto {
  region: NormRectDto;
  scene: number | null;
  total: number | null;
  labelTemplateLoaded: boolean;
  labelThreshold: number;
  digitThreshold: number;
  anchorScore: number;
  anchorBox: NormRectDto | null;
  stripRegion: NormRectDto | null;
  candidates: DigitMatchDto[];
  kept: DigitMatchDto[];
  splitAt: number | null;
  bestGap: number;
  avgWidth: number;
  trimmedLeft: number;
  trimmedRight: number;
  missingDigitTemplates: number[];
  failReason: string | null;
}

const BATTLE_SCENE_FAIL_HINTS: Record<string, string> = {
  empty_region: "区域为空（NormRect 越界？）",
  missing_label_template: "缺少 text_battle_label 模板",
  region_smaller_than_label: "区域比锚点模板还小，请扩大",
  anchor_below_threshold: "未找到 BATTLE 锚点（被遮挡或区域偏移）",
  strip_too_narrow: "锚点右侧空间不足以容纳数字",
  no_digit_candidates: "右侧未匹配到任何数字（阈值 ≥ 0.8）",
  fewer_than_two_digits: "数字不足 2 个（无法切分 m/n）",
  no_separator_gap: "数字间无足够间隔（找不到斜杠位置）",
  cohesion_trim_emptied_side: "切分后某一侧无有效数字（误检过多）",
  parse_error: "数字解析失败",
};

interface SupportCeInfoDto {
  region: NormRectDto;
  score: number;
  passed: boolean;
  threshold: number;
  templatePath?: string;
  error?: string;
}

interface SupportRowMatchDto {
  rowRegion: NormRectDto;
  tap: PointDto;
  nameText: string;
  nameScore: number;
  nameRegion: NormRectDto;
  npText: string;
  npScore: number;
  npRegion: NormRectDto;
  npMatchedName: string;
  /**
   * Per-row craft-essence verification, populated only when the user
   * supplies a CE id in the debug toolbar. Lets the overlay draw the
   * CE search window and the score so `SUPPORT_CE_OFFSET_IN_ROW` /
   * `SUPPORT_CE_THRESHOLD` can be calibrated against real captures.
   */
  ce?: SupportCeInfoDto;
}

interface SupportCandidateDto {
  text: string;
  score: number;
  region: NormRectDto;
  matchedName?: string;
}

interface SupportDiagnosticsDto {
  listRegion: NormRectDto;
  nameCandidates: SupportCandidateDto[];
  npCandidates: SupportCandidateDto[];
  fragmentCount: number;
}

interface FindSupportsResultDto {
  supports: SupportRowMatchDto[];
  diagnostics: SupportDiagnosticsDto;
}

interface ServantMetadataDto {
  id: number;
  name: string;
  npNames: string[];
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

  const [coordinates, setCoordinates] = useState<RunnerCoordinatesDto | null>(
    null
  );
  const [showCoordOverlay, setShowCoordOverlay] = useState(false);
  const [visibleCoordGroups, setVisibleCoordGroups] = useState<Set<string>>(
    new Set()
  );

  const [availableServantIds, setAvailableServantIds] = useState<number[]>([]);
  const [cardServantInput, setCardServantInput] = useState("");
  const [commandCards, setCommandCards] = useState<CommandCardMatchDto[]>([]);
  const [findingCards, setFindingCards] = useState(false);
  const [noblePhantasms, setNoblePhantasms] = useState<NoblePhantasmMatchDto[]>(
    []
  );
  const [findingNps, setFindingNps] = useState(false);
  const [battleScene, setBattleScene] =
    useState<BattleSceneResultDto | null>(null);
  const [readingBattleScene, setReadingBattleScene] = useState(false);
  const [supportServantId, setSupportServantId] = useState<string>("");
  const [supportCraftEssenceId, setSupportCraftEssenceId] = useState<string>("");
  const [supportMetadata, setSupportMetadata] =
    useState<ServantMetadataDto | null>(null);
  const [supportResult, setSupportResult] =
    useState<FindSupportsResultDto | null>(null);
  const [findingSupports, setFindingSupports] = useState(false);
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

  const loadAvailableServantIds = useCallback(async () => {
    try {
      const ids = await invoke<number[]>("debug_list_servant_assets");
      setAvailableServantIds(ids);
      log(
        `已加载 ${ids.length} 个从者素材${ids.length > 0 ? ": " + ids.slice(0, 8).join(", ") + (ids.length > 8 ? " …" : "") : ""}`
      );
    } catch (err) {
      log(`读取从者素材失败: ${err}`, "error");
    }
  }, [log]);

  const loadCoordinates = useCallback(async () => {
    try {
      const coords = await invoke<RunnerCoordinatesDto>(
        "debug_get_runner_coordinates"
      );
      setCoordinates(coords);
      log(
        `已加载坐标分组: ${coords.groups.map((g) => g.label).join(", ") || "(空)"}`
      );
    } catch (err) {
      log(`读取坐标配置失败: ${err}`, "error");
    }
  }, [log]);

  useEffect(() => {
    log("初始化 CV 调试页面");
    loadConfig();
    loadTemplateList();
    loadCoordinates();
    loadAvailableServantIds();
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
      setCommandCards([]);
      setNoblePhantasms([]);
      setBattleScene(null);
      setSupportResult(null);
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
    setCommandCards([]);
    setNoblePhantasms([]);
    setBattleScene(null);
    setSupportResult(null);
    log("已清除标注");
  }, [log]);

  const parsedCardServantIds = useMemo<number[]>(() => {
    const seen = new Set<number>();
    const out: number[] = [];
    for (const tok of cardServantInput.split(/[\s,，]+/)) {
      const n = Number(tok.trim());
      if (Number.isFinite(n) && n > 0 && !seen.has(n)) {
        seen.add(n);
        out.push(n);
      }
    }
    return out;
  }, [cardServantInput]);

  const handleFindCommandCards = useCallback(async () => {
    if (!capture) return;
    setFindingCards(true);
    log(
      `调用 debug_find_command_cards (servantIds=[${parsedCardServantIds.join(", ")}])`
    );
    try {
      const cards = await invoke<CommandCardMatchDto[]>(
        "debug_find_command_cards",
        { servantIds: parsedCardServantIds }
      );
      setCommandCards(cards);
      if (cards.length === 0) {
        log("未识别到指令卡 — 检查截图是否为攻击画面", "warn");
      } else {
        const summary = cards
          .map((c) => {
            const ident = c.servantId
              ? ` ${c.servantId}@${c.ascension ?? "?"}(${(c.faceScore ?? 0).toFixed(2)})`
              : "";
            return `C${c.slot + 1}:${c.suit}${ident}`;
          })
          .join("  ");
        log(`识别到 ${cards.length} 张指令卡 | ${summary}`);
      }
    } catch (err) {
      log(`debug_find_command_cards 失败: ${err}`, "error");
    } finally {
      setFindingCards(false);
    }
  }, [capture, parsedCardServantIds, log]);

  const handleFindNoblePhantasms = useCallback(async () => {
    if (!capture) return;
    setFindingNps(true);
    log("调用 debug_find_noble_phantasms");
    try {
      const slots = await invoke<NoblePhantasmMatchDto[]>(
        "debug_find_noble_phantasms"
      );
      setNoblePhantasms(slots);
      const readyCount = slots.filter((s) => s.ready).length;
      const summary = slots
        .map(
          (s) =>
            `NP${s.slot + 1}:${s.ready ? "有" : "无"}(${(s.edgeFrac * 100).toFixed(1)}%)`
        )
        .join("  ");
      log(`识别到 ${readyCount}/${slots.length} 张宝具卡 | ${summary}`);
    } catch (err) {
      log(`debug_find_noble_phantasms 失败: ${err}`, "error");
    } finally {
      setFindingNps(false);
    }
  }, [capture, log]);

  const handleReadBattleScene = useCallback(async () => {
    if (!capture) return;
    setReadingBattleScene(true);
    log("调用 debug_read_battle_scene");
    try {
      const result = await invoke<BattleSceneResultDto>(
        "debug_read_battle_scene"
      );
      setBattleScene(result);
      const anchor = `锚点 ${(result.anchorScore * 100).toFixed(1)}% (阈值 ${(
        result.labelThreshold * 100
      ).toFixed(0)}%)`;
      const digits =
        result.kept.length > 0
          ? `命中数字 [${result.kept
              .map((d) => `${d.value}@${(d.score * 100).toFixed(0)}%`)
              .join(", ")}]`
          : `数字 0 个 (阈值 ${(result.digitThreshold * 100).toFixed(0)}%)`;
      const trimNote =
        result.trimmedLeft + result.trimmedRight > 0
          ? ` | 已剔除 ${result.trimmedLeft + result.trimmedRight} 个噪声数字`
          : "";
      if (result.scene !== null && result.total !== null) {
        log(
          `战斗场景: ${result.scene}/${result.total} → 第 ${result.scene} 组指令 | ${anchor} | ${digits}${trimNote}`
        );
      } else {
        const reason = result.failReason ?? "unknown";
        const hint = BATTLE_SCENE_FAIL_HINTS[reason] ?? reason;
        log(
          `战斗场景: 未识别 (${hint}) | ${anchor} | ${digits}`,
          "warn"
        );
        if (result.missingDigitTemplates.length > 0) {
          log(
            `缺失数字模板: ${result.missingDigitTemplates.join(", ")}`,
            "warn"
          );
        }
      }
    } catch (err) {
      log(`debug_read_battle_scene 失败: ${err}`, "error");
    } finally {
      setReadingBattleScene(false);
    }
  }, [capture, log]);

  const handleUseAllAvailableIds = useCallback(() => {
    setCardServantInput(availableServantIds.join(", "));
  }, [availableServantIds]);

  const parsedSupportServantId = useMemo<number | null>(() => {
    const n = Number(supportServantId.trim());
    return Number.isFinite(n) && n > 0 ? n : null;
  }, [supportServantId]);

  const parsedSupportCraftEssenceId = useMemo<number | null>(() => {
    const raw = supportCraftEssenceId.trim();
    if (!raw) return null;
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? n : null;
  }, [supportCraftEssenceId]);

  const handleFindSupports = useCallback(async () => {
    if (!capture || parsedSupportServantId === null) return;
    setFindingSupports(true);
    log(`调用 debug_find_supports (servantId=${parsedSupportServantId})`);
    try {
      // Fetch metadata first so the log + side panel can show what we
      // actually fed the OCR detector (helpful when a row misses).
      let meta = supportMetadata;
      if (!meta || meta.id !== parsedSupportServantId) {
        meta = await invoke<ServantMetadataDto>("get_servant_metadata", {
          id: parsedSupportServantId,
        });
        setSupportMetadata(meta);
        log(
          `加载从者元数据: ${meta.name} · 宝具 [${meta.npNames.join(" / ")}]`
        );
      }
      const result = await invoke<FindSupportsResultDto>(
        "debug_find_supports",
        {
          servantId: parsedSupportServantId,
          craftEssenceId: parsedSupportCraftEssenceId,
        }
      );
      setSupportResult(result);
      const diag = result.diagnostics;
      log(
        `识别到 ${result.supports.length} 行助战 | OCR ${diag.fragmentCount} 片段` +
          ` · 名称候选 ${diag.nameCandidates.length}` +
          ` · 宝具候选 ${diag.npCandidates.length}`
      );
      if (result.supports.length === 0) {
        log("未匹配到助战行 — 检查截图是否为助战选择画面", "warn");
      } else {
        for (const s of result.supports) {
          const cePart = s.ce
            ? ` | 礼装 ${s.ce.score.toFixed(2)}/${s.ce.threshold.toFixed(2)} ${s.ce.passed ? "✓" : "✗"}`
            : "";
          log(
            `  行 y=${s.rowRegion.y.toFixed(3)} | 名称='${s.nameText}' (${s.nameScore.toFixed(2)})` +
              ` | 宝具='${s.npText}' (${s.npScore.toFixed(2)})` +
              cePart
          );
        }
      }
    } catch (err) {
      log(`debug_find_supports 失败: ${err}`, "error");
    } finally {
      setFindingSupports(false);
    }
  }, [
    capture,
    parsedSupportServantId,
    parsedSupportCraftEssenceId,
    supportMetadata,
    log,
  ]);

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
        <Flex direction="column" className="debug-canvas-col" gap="2">
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
              disabled={
                probes.length === 0 &&
                commandCards.length === 0 &&
                noblePhantasms.length === 0 &&
                supportResult === null
              }
              onClick={handleClearOverlays}
            >
              清除标注
            </button>
          </Flex>

          <DebugSection title="原始模板探针">
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
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
          </DebugSection>

          <DebugSection title="指令卡 / 宝具卡识别">
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <input
                className="debug-input"
                placeholder="候选从者 id (逗号分隔，可留空只定位卡槽)"
                value={cardServantInput}
                onChange={(e) => setCardServantInput(e.target.value)}
                style={{ flex: 1, minWidth: 240 }}
              />
              <button
                className="battle-btn battle-btn-start debug-btn-small"
                disabled={availableServantIds.length === 0}
                onClick={handleUseAllAvailableIds}
                title={`填入全部 ${availableServantIds.length} 个有素材的从者 id`}
              >
                全选 ({availableServantIds.length})
              </button>
              <button
                className="battle-btn battle-btn-start debug-btn-small"
                disabled={findingCards || !capture}
                onClick={handleFindCommandCards}
              >
                {findingCards ? "识别中…" : "识别指令卡"}
              </button>
              <button
                className="battle-btn battle-btn-start debug-btn-small"
                disabled={findingNps || !capture}
                onClick={handleFindNoblePhantasms}
              >
                {findingNps ? "识别中…" : "识别宝具卡"}
              </button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="战斗场景识别"
            badge={
              battleScene && battleScene.scene !== null && battleScene.total !== null
                ? `${battleScene.scene}/${battleScene.total}`
                : battleScene
                  ? "未识别"
                  : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Text size="2" color="gray">
                读取右上角 BATTLE m/n，用于挑选第 m 组指令配置。
              </Text>
              <button
                className="battle-btn battle-btn-start debug-btn-small"
                disabled={readingBattleScene || !capture}
                onClick={handleReadBattleScene}
              >
                {readingBattleScene ? "识别中…" : "识别战斗场景"}
              </button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="助战识别"
            badge={
              supportMetadata
                ? `${supportMetadata.name} · ${supportMetadata.npNames.length} 宝具`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <input
                className="debug-input"
                list="debug-support-servant-list"
                placeholder="助战从者 id"
                value={supportServantId}
                onChange={(e) => setSupportServantId(e.target.value)}
                style={{ width: 140 }}
              />
              <datalist id="debug-support-servant-list">
                {availableServantIds.map((id) => (
                  <option key={`support-id-${id}`} value={id} />
                ))}
              </datalist>
              <input
                className="debug-input"
                placeholder="礼装 id (可选)"
                value={supportCraftEssenceId}
                onChange={(e) => setSupportCraftEssenceId(e.target.value)}
                style={{ width: 140 }}
                title="留空跳过礼装识别。提供后会按行运行 verify_support_ce 并叠加搜索框 + 分数。"
              />
              <button
                className="battle-btn battle-btn-start debug-btn-small"
                disabled={
                  findingSupports || !capture || parsedSupportServantId === null
                }
                onClick={handleFindSupports}
              >
                {findingSupports ? "识别中…" : "识别助战"}
              </button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="坐标叠层"
            badge={
              showCoordOverlay
                ? `${visibleCoordGroups.size} / ${coordinates?.groups.length ?? 0}`
                : "off"
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <label className="debug-coord-toggle">
                <input
                  type="checkbox"
                  checked={showCoordOverlay}
                  onChange={(e) => setShowCoordOverlay(e.target.checked)}
                />
                <Text size="1">坐标叠层</Text>
              </label>
              {coordinates?.groups.map((g) => (
                <label key={g.id} className="debug-coord-toggle">
                  <input
                    type="checkbox"
                    disabled={!showCoordOverlay}
                    checked={visibleCoordGroups.has(g.id)}
                    onChange={(e) => {
                      setVisibleCoordGroups((prev) => {
                        const next = new Set(prev);
                        if (e.target.checked) next.add(g.id);
                        else next.delete(g.id);
                        return next;
                      });
                    }}
                  />
                  <Text size="1" color={showCoordOverlay ? undefined : "gray"}>
                    {g.label}
                  </Text>
                </label>
              ))}
            </Flex>
          </DebugSection>

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
                {commandCards.flatMap((c) => {
                  const overlays = [
                    <Box
                      key={`card-face-${c.slot}`}
                      className="debug-overlay-box debug-overlay-face"
                      style={{
                        left: `${c.faceRegion.x * 100}%`,
                        top: `${c.faceRegion.y * 100}%`,
                        width: `${c.faceRegion.w * 100}%`,
                        height: `${c.faceRegion.h * 100}%`,
                      }}
                    />,
                    <Box
                      key={`card-slot-${c.slot}`}
                      className="debug-overlay-box debug-overlay-card"
                      style={{
                        left: `${c.cardRegion.x * 100}%`,
                        top: `${c.cardRegion.y * 100}%`,
                        width: `${c.cardRegion.w * 100}%`,
                        height: `${c.cardRegion.h * 100}%`,
                      }}
                    >
                      <span className="debug-overlay-label">
                        C{c.slot + 1}
                        {c.suit ? `·${c.suit.toUpperCase()}` : ""}
                        {c.servantId !== undefined
                          ? ` · ${c.servantId}@${c.ascension ?? "?"} (${(c.faceScore ?? 0).toFixed(2)})`
                          : c.iconScore !== undefined
                            ? ` · ${c.iconScore.toFixed(2)}`
                            : ""}
                      </span>
                    </Box>,
                  ];
                  if (c.iconRegion) {
                    overlays.push(
                      <Box
                        key={`card-icon-${c.slot}`}
                        className="debug-overlay-box debug-overlay-icon"
                        style={{
                          left: `${c.iconRegion.x * 100}%`,
                          top: `${c.iconRegion.y * 100}%`,
                          width: `${c.iconRegion.w * 100}%`,
                          height: `${c.iconRegion.h * 100}%`,
                        }}
                      />
                    );
                  }
                  return overlays;
                })}
                {noblePhantasms.map((s) => (
                  <Box
                    key={`np-slot-${s.slot}`}
                    className={`debug-overlay-box ${
                      s.ready
                        ? "debug-overlay-np-ready"
                        : "debug-overlay-np-empty"
                    }`}
                    style={{
                      left: `${s.cardRegion.x * 100}%`,
                      top: `${s.cardRegion.y * 100}%`,
                      width: `${s.cardRegion.w * 100}%`,
                      height: `${s.cardRegion.h * 100}%`,
                    }}
                  >
                    <span className="debug-overlay-label">
                      NP{s.slot + 1} · {s.ready ? "ready" : "empty"} · edge{" "}
                      {(s.edgeFrac * 100).toFixed(1)}% · std{" "}
                      {s.stdBgr.toFixed(0)}
                    </span>
                  </Box>
                ))}
                {battleScene && (
                  <>
                    <Box
                      key="battle-scene-region"
                      className={`debug-overlay-box ${
                        battleScene.scene !== null
                          ? "debug-overlay-np-ready"
                          : "debug-overlay-np-empty"
                      }`}
                      style={{
                        left: `${battleScene.region.x * 100}%`,
                        top: `${battleScene.region.y * 100}%`,
                        width: `${battleScene.region.w * 100}%`,
                        height: `${battleScene.region.h * 100}%`,
                      }}
                    >
                      <span className="debug-overlay-label">
                        战斗场景{" "}
                        {battleScene.scene !== null &&
                        battleScene.total !== null
                          ? `${battleScene.scene}/${battleScene.total}`
                          : "未识别"}
                      </span>
                    </Box>
                    {battleScene.anchorBox && (
                      <Box
                        key="battle-scene-anchor"
                        className="debug-overlay-box debug-overlay-support-region"
                        style={{
                          left: `${battleScene.anchorBox.x * 100}%`,
                          top: `${battleScene.anchorBox.y * 100}%`,
                          width: `${battleScene.anchorBox.w * 100}%`,
                          height: `${battleScene.anchorBox.h * 100}%`,
                        }}
                      >
                        <span className="debug-overlay-label">
                          BATTLE 锚点 {(battleScene.anchorScore * 100).toFixed(0)}
                          %
                        </span>
                      </Box>
                    )}
                    {battleScene.candidates.map((d, i) => {
                      const isKept = battleScene.kept.some(
                        (k) =>
                          k.value === d.value &&
                          Math.abs(k.region.x - d.region.x) < 1e-6 &&
                          Math.abs(k.region.y - d.region.y) < 1e-6
                      );
                      return (
                        <Box
                          key={`battle-scene-digit-${i}`}
                          className={`debug-overlay-box ${
                            isKept
                              ? "debug-overlay-support-name-cand"
                              : "debug-overlay-support-np-cand"
                          }`}
                          style={{
                            left: `${d.region.x * 100}%`,
                            top: `${d.region.y * 100}%`,
                            width: `${d.region.w * 100}%`,
                            height: `${d.region.h * 100}%`,
                          }}
                        >
                          <span className="debug-overlay-label">
                            {d.value} · {(d.score * 100).toFixed(0)}%
                            {isKept ? "" : " (drop)"}
                          </span>
                        </Box>
                      );
                    })}
                  </>
                )}
                {supportResult && (
                  <>
                    <Box
                      key="support-list-region"
                      className="debug-overlay-box debug-overlay-support-region"
                      style={{
                        left: `${supportResult.diagnostics.listRegion.x * 100}%`,
                        top: `${supportResult.diagnostics.listRegion.y * 100}%`,
                        width: `${supportResult.diagnostics.listRegion.w * 100}%`,
                        height: `${supportResult.diagnostics.listRegion.h * 100}%`,
                      }}
                    >
                      <span className="debug-overlay-label">助战列表</span>
                    </Box>
                    {supportResult.diagnostics.nameCandidates.map((c, i) => (
                      <Box
                        key={`support-name-cand-${i}`}
                        className="debug-overlay-box debug-overlay-support-name-cand"
                        style={{
                          left: `${c.region.x * 100}%`,
                          top: `${c.region.y * 100}%`,
                          width: `${c.region.w * 100}%`,
                          height: `${c.region.h * 100}%`,
                        }}
                      >
                        <span className="debug-overlay-label">
                          名 {c.score.toFixed(2)} · {c.text}
                        </span>
                      </Box>
                    ))}
                    {supportResult.diagnostics.npCandidates.map((c, i) => (
                      <Box
                        key={`support-np-cand-${i}`}
                        className="debug-overlay-box debug-overlay-support-np-cand"
                        style={{
                          left: `${c.region.x * 100}%`,
                          top: `${c.region.y * 100}%`,
                          width: `${c.region.w * 100}%`,
                          height: `${c.region.h * 100}%`,
                        }}
                      >
                        <span className="debug-overlay-label">
                          宝 {c.score.toFixed(2)} · {c.text}
                          {c.matchedName ? ` (${c.matchedName})` : ""}
                        </span>
                      </Box>
                    ))}
                    {supportResult.supports.map((s, i) => (
                      <Box
                        key={`support-row-${i}`}
                        className="debug-overlay-box debug-overlay-support-row"
                        style={{
                          left: `${s.rowRegion.x * 100}%`,
                          top: `${s.rowRegion.y * 100}%`,
                          width: `${s.rowRegion.w * 100}%`,
                          height: `${s.rowRegion.h * 100}%`,
                        }}
                      >
                        <span className="debug-overlay-label">
                          助战 {i + 1} · 名 {s.nameScore.toFixed(2)} · 宝{" "}
                          {s.npScore.toFixed(2)}
                        </span>
                      </Box>
                    ))}
                    {supportResult.supports.map((s, i) => (
                      <Box
                        key={`support-tap-${i}`}
                        className="debug-coord-dot debug-overlay-support-tap"
                        style={{
                          left: `${s.tap.x * 100}%`,
                          top: `${s.tap.y * 100}%`,
                        }}
                        title={`tap (${s.tap.x.toFixed(3)}, ${s.tap.y.toFixed(3)})`}
                      >
                        <span className="debug-coord-label">点击 {i + 1}</span>
                      </Box>
                    ))}
                    {supportResult.supports.map((s, i) =>
                      s.ce ? (
                        <Box
                          key={`support-ce-${i}`}
                          className="debug-overlay-box debug-overlay-support-ce"
                          style={{
                            left: `${s.ce.region.x * 100}%`,
                            top: `${s.ce.region.y * 100}%`,
                            width: `${s.ce.region.w * 100}%`,
                            height: `${s.ce.region.h * 100}%`,
                            outline: `2px solid ${s.ce.passed ? "#3fb950" : "#f85149"}`,
                          }}
                          title={
                            s.ce.error
                              ? `CE verify error: ${s.ce.error}`
                              : `CE score ${s.ce.score.toFixed(3)} (threshold ${s.ce.threshold.toFixed(2)})`
                          }
                        >
                          <span className="debug-overlay-label">
                            礼 {s.ce.score.toFixed(2)}/{s.ce.threshold.toFixed(2)}{" "}
                            {s.ce.passed ? "✓" : "✗"}
                          </span>
                        </Box>
                      ) : null
                    )}
                  </>
                )}
                {showCoordOverlay &&
                  coordinates?.groups
                    .filter((g) => visibleCoordGroups.has(g.id))
                    .flatMap((g) => [
                      ...g.regions.map((r) => (
                        <Box
                          key={`coord-region-${g.id}-${r.label}`}
                          className="debug-coord-region"
                          style={{
                            left: `${r.region.x * 100}%`,
                            top: `${r.region.y * 100}%`,
                            width: `${r.region.w * 100}%`,
                            height: `${r.region.h * 100}%`,
                          }}
                          title={`${g.label} · ${r.label}`}
                        >
                          <span className="debug-coord-label">
                            {g.label} · {r.label}
                          </span>
                        </Box>
                      )),
                      ...g.points.map((p) => (
                        <Box
                          key={`coord-dot-${g.id}-${p.label}`}
                          className="debug-coord-dot"
                          style={{
                            left: `${p.point.x * 100}%`,
                            top: `${p.point.y * 100}%`,
                          }}
                          title={`${g.label} · ${p.label} (${p.point.x.toFixed(3)}, ${p.point.y.toFixed(3)})`}
                        >
                          <span className="debug-coord-label">{p.label}</span>
                        </Box>
                      )),
                    ])}
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
            <DebugSection title="当前元素配置">
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
            </DebugSection>
          )}

          <DebugSection
            title="指令卡识别"
            badge={commandCards.length || undefined}
          >
            <Box className="debug-match-list">
              {commandCards.length === 0 && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {commandCards.map((c) => (
                <Box
                  key={`card-row-${c.slot}`}
                  className={`debug-match-entry ${c.servantId !== undefined ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      C{c.slot + 1}
                      {c.suit ? ` · ${c.suit.toUpperCase()}` : " · ?"}
                    </Text>
                    <Text size="1" color="gray">
                      {c.iconScore !== undefined
                        ? `icon ${c.iconScore.toFixed(3)}`
                        : "无图标"}
                    </Text>
                  </Flex>
                  {c.servantId !== undefined ? (
                    <Text size="1" color="green">
                      从者 {c.servantId} · 进阶 {c.ascension ?? "?"} · face{" "}
                      {(c.faceScore ?? 0).toFixed(3)}
                    </Text>
                  ) : (
                    <Text size="1" color="gray">
                      未识别从者
                    </Text>
                  )}
                  <Text size="1" color="gray">
                    中心 ({c.x.toFixed(3)}, {c.y.toFixed(3)})
                  </Text>
                </Box>
              ))}
            </Box>
          </DebugSection>

          <DebugSection
            title="宝具卡识别"
            badge={
              noblePhantasms.length > 0
                ? `${noblePhantasms.filter((s) => s.ready).length}/${noblePhantasms.length}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {noblePhantasms.length === 0 && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {noblePhantasms.map((s) => (
                <Box
                  key={`np-row-${s.slot}`}
                  className={`debug-match-entry ${s.ready ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      NP{s.slot + 1}
                    </Text>
                    <Text size="1" color={s.ready ? "green" : "gray"}>
                      {s.ready ? "ready" : "empty"}
                    </Text>
                  </Flex>
                  <Text size="1" color="gray">
                    edge {(s.edgeFrac * 100).toFixed(2)}% · std{" "}
                    {s.stdBgr.toFixed(1)}
                  </Text>
                </Box>
              ))}
            </Box>
          </DebugSection>

          <DebugSection
            title="助战识别"
            badge={supportResult?.supports.length || undefined}
          >
            <Box className="debug-match-list">
              {!supportResult && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {supportResult && supportMetadata && (
                <Box className="debug-match-entry">
                  <Text size="2" weight="medium">
                    目标：{supportMetadata.name} (#{supportMetadata.id})
                  </Text>
                  <Text size="1" color="gray">
                    宝具候选: {supportMetadata.npNames.join(" / ")}
                  </Text>
                  <Text size="1" color="gray">
                    OCR 片段 {supportResult.diagnostics.fragmentCount} · 名称候选{" "}
                    {supportResult.diagnostics.nameCandidates.length} ·
                    宝具候选 {supportResult.diagnostics.npCandidates.length}
                  </Text>
                </Box>
              )}
              {supportResult?.supports.map((s, i) => (
                <Box
                  key={`support-side-${i}`}
                  className="debug-match-entry found"
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      助战 {i + 1}
                    </Text>
                    <Text size="1" color="gray">
                      tap ({s.tap.x.toFixed(3)}, {s.tap.y.toFixed(3)})
                    </Text>
                  </Flex>
                  <Text size="1" color="green">
                    名: {s.nameText} ({s.nameScore.toFixed(2)})
                  </Text>
                  <Text size="1" color="green">
                    宝: {s.npText} ({s.npScore.toFixed(2)})
                    {s.npMatchedName ? ` → ${s.npMatchedName}` : ""}
                  </Text>
                  <Text size="1" color="gray">
                    行 y={s.rowRegion.y.toFixed(3)} h=
                    {s.rowRegion.h.toFixed(3)}
                  </Text>
                  {s.ce && (
                    <Text
                      size="1"
                      color={s.ce.passed ? "green" : "red"}
                    >
                      礼装 {s.ce.score.toFixed(3)} /{" "}
                      {s.ce.threshold.toFixed(2)}{" "}
                      {s.ce.passed ? "✓ 匹配" : "✗ 未达阈值"}
                      {s.ce.error ? ` · ${s.ce.error}` : ""}
                    </Text>
                  )}
                </Box>
              ))}
              {supportResult && supportResult.supports.length === 0 && (
                <>
                  {supportResult.diagnostics.nameCandidates.length > 0 && (
                    <Box className="debug-match-entry missed">
                      <Text size="1" color="gray">
                        名称候选 (未配对):
                      </Text>
                      {supportResult.diagnostics.nameCandidates
                        .slice(0, 8)
                        .map((c, i) => (
                          <Text
                            size="1"
                            color="gray"
                            key={`name-cand-side-${i}`}
                          >
                            · {c.text} ({c.score.toFixed(2)}) @ y=
                            {c.region.y.toFixed(3)}
                          </Text>
                        ))}
                    </Box>
                  )}
                  {supportResult.diagnostics.npCandidates.length > 0 && (
                    <Box className="debug-match-entry missed">
                      <Text size="1" color="gray">
                        宝具候选 (未配对):
                      </Text>
                      {supportResult.diagnostics.npCandidates
                        .slice(0, 8)
                        .map((c, i) => (
                          <Text size="1" color="gray" key={`np-cand-side-${i}`}>
                            · {c.text} ({c.score.toFixed(2)})
                            {c.matchedName ? ` → ${c.matchedName}` : ""} @ y=
                            {c.region.y.toFixed(3)}
                          </Text>
                        ))}
                    </Box>
                  )}
                </>
              )}
            </Box>
          </DebugSection>

          <DebugSection
            title="匹配历史"
            badge={probes.length || undefined}
          >
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
          </DebugSection>
        </Flex>
      </Flex>
    </Flex>
  );
}
