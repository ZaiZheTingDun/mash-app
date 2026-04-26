import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  ExternalLinkIcon,
  ReloadIcon,
} from "@radix-ui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { CvConfig } from "../types/cv";
import type { DebugCanvasState } from "./DebugCanvas";
import {
  DEBUG_CANVAS_REQUEST_EVENT,
  DEBUG_CANVAS_STATE_EVENT,
} from "./DebugCanvasWindow";

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

export interface DebugScreenSize {
  w: number;
  h: number;
}

export interface DebugCaptureResult {
  imagePath: string;
  screen: string;
  score: number;
  screenSize: DebugScreenSize | null;
}

export interface NormRectDto {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ElementMatchDto {
  found: boolean;
  x: number;
  y: number;
  score: number;
  region: NormRectDto | null;
}

export interface ProbeResult {
  label: string;
  threshold: number;
  match: ElementMatchDto;
  timestamp: string;
}

export interface PointDto {
  x: number;
  y: number;
}

export interface LabeledPointDto {
  label: string;
  point: PointDto;
}

export interface LabeledRegionDto {
  label: string;
  region: NormRectDto;
}

export interface CoordGroupDto {
  id: string;
  label: string;
  points: LabeledPointDto[];
  regions: LabeledRegionDto[];
}

export interface RunnerCoordinatesDto {
  groups: CoordGroupDto[];
}

export interface CommandCardMatchDto {
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

export interface NoblePhantasmMatchDto {
  slot: number;
  cardRegion: NormRectDto;
  ready: boolean;
  edgeFrac: number;
  stdBgr: number;
  edgeThreshold?: number;
}

export interface DigitMatchDto {
  value: number;
  score: number;
  region: NormRectDto;
}

/**
 * Snapshot of the runner's attack-button probe (template
 * `button_attack` inside `ATTACK_BUTTON_REGION` at threshold
 * `ATTACK_BUTTON_THRESHOLD`). Surfaces both runner constants and the
 * live match score so the user can tell why automation is stuck on
 * "等待战斗动作…".
 */
export interface AttackButtonResultDto {
  template: string;
  region: NormRectDto;
  threshold: number;
  tapPoint: PointDto;
  found: boolean;
  score: number;
  matchX: number;
  matchY: number;
  matchRegion: NormRectDto | null;
}

export interface BattleSceneResultDto {
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

export interface SupportCeInfoDto {
  region: NormRectDto;
  score: number;
  passed: boolean;
  threshold: number;
  templatePath?: string;
  error?: string;
}

export interface SupportRowMatchDto {
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

export interface SupportCandidateDto {
  text: string;
  score: number;
  region: NormRectDto;
  matchedName?: string;
}

export interface SupportFragmentDto {
  text: string;
  region: NormRectDto;
  ocrConfidence: number;
  nameScore: number;
  bestNpScore: number;
  bestNpName: string;
}

export interface SupportDiagnosticsDto {
  listRegion: NormRectDto;
  nameCandidates: SupportCandidateDto[];
  npCandidates: SupportCandidateDto[];
  fragmentCount: number;
  fragments?: SupportFragmentDto[];
  nameOnlyFallback?: boolean;
  nameOnlyReason?: string;
}

export interface FindSupportsResultDto {
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
  const [attackButton, setAttackButton] =
    useState<AttackButtonResultDto | null>(null);
  const [findingAttackButton, setFindingAttackButton] = useState(false);
  const [supportServantId, setSupportServantId] = useState<string>("");
  const [supportCraftEssenceId, setSupportCraftEssenceId] = useState<string>("");
  const [supportMetadata, setSupportMetadata] =
    useState<ServantMetadataDto | null>(null);
  const [supportResult, setSupportResult] =
    useState<FindSupportsResultDto | null>(null);
  const [findingSupports, setFindingSupports] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);
  const didShutdown = useRef(false);
  const [popoutOpen, setPopoutOpen] = useState(false);

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
      setAttackButton(null);
      setSupportResult(null);
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
    setAttackButton(null);
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
      const threshold = slots[0]?.edgeThreshold;
      const thresholdHint =
        threshold !== undefined
          ? ` · 阈值 ${(threshold * 100).toFixed(1)}%`
          : "";
      log(
        `识别到 ${readyCount}/${slots.length} 张宝具卡${thresholdHint} | ${summary}`
      );
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

  const handleFindAttackButton = useCallback(async () => {
    if (!capture) return;
    setFindingAttackButton(true);
    log("调用 debug_find_attack_button");
    try {
      const result = await invoke<AttackButtonResultDto>(
        "debug_find_attack_button"
      );
      setAttackButton(result);
      const scorePct = (result.score * 100).toFixed(1);
      const thresholdPct = (result.threshold * 100).toFixed(0);
      if (result.found) {
        log(
          `攻击按钮: ✓ ${scorePct}% / ${thresholdPct}% | 中心 (${result.matchX.toFixed(3)}, ${result.matchY.toFixed(3)}) | 点击 (${result.tapPoint.x.toFixed(3)}, ${result.tapPoint.y.toFixed(3)})`
        );
      } else {
        log(
          `攻击按钮: ✗ ${scorePct}% < ${thresholdPct}% | 模板 ${result.template}`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_find_attack_button 失败: ${err}`, "error");
    } finally {
      setFindingAttackButton(false);
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
      if (diag.nameOnlyFallback) {
        // Surface the degraded path immediately instead of waiting for
        // the user to expand the diagnostics panel — when this fires
        // the rows are real but their CE / NP cross-check was skipped.
        const why =
          diag.nameOnlyReason === "noNpExpected"
            ? "宝具中文翻译未映射（CN servants.json 缺失）"
            : diag.nameOnlyReason === "noNpAboveThreshold"
              ? "OCR 未找到匹配的宝具文本（mooncell 译名与游戏内不一致）"
              : "回退到仅按名称识别";
        log(`仅按名称识别（已跳过宝具核对）: ${why}`, "warn");
      }
      if (result.supports.length === 0) {
        log("未匹配到助战行 — 检查截图是否为助战选择画面", "warn");
        // When OCR ran but nothing matched, dump the closest 5 fragments
        // by best-NP-score so the user can immediately see what OCR
        // actually read in the NP region. Replaces the "lower the
        // threshold and re-run" round-trip.
        const frags = (diag.fragments ?? [])
          .filter((f) => f.text.trim().length > 0)
          .sort((a, b) => b.bestNpScore - a.bestNpScore)
          .slice(0, 5);
        for (const f of frags) {
          log(
            `  片段 '${f.text}' | 名称分 ${f.nameScore.toFixed(2)}` +
              ` | 宝具最佳 ${f.bestNpScore.toFixed(2)}` +
              (f.bestNpName ? ` (vs '${f.bestNpName}')` : "")
          );
        }
      } else {
        for (const s of result.supports) {
          const cePart = s.ce
            ? ` | 礼装 ${s.ce.score.toFixed(2)}/${s.ce.threshold.toFixed(2)} ${s.ce.passed ? "✓" : "✗"}`
            : "";
          const npPart = s.npText
            ? ` | 宝具='${s.npText}' (${s.npScore.toFixed(2)})`
            : ` | 宝具(未核对)`;
          log(
            `  行 y=${s.rowRegion.y.toFixed(3)} | 名称='${s.nameText}' (${s.nameScore.toFixed(2)})` +
              npPart +
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

  /**
   * Spawn (or focus, if already open) the popout `WebviewWindow` that
   * mirrors the canvas at full size. The popout loads the same SPA
   * bundle with `#debug-canvas` in the URL hash; `main.tsx` swaps in
   * `<DebugCanvasWindow />` based on that hash. State sync runs over
   * Tauri events (`debug-canvas:state` / `debug-canvas:request`) — see
   * the popout-sync `useEffect`s above.
   */
  const handleOpenPopout = useCallback(async () => {
    try {
      const existing = await WebviewWindow.getByLabel("debug-canvas");
      if (existing) {
        await existing.show();
        await existing.setFocus();
        log("已聚焦弹出画面");
        return;
      }
      const win = new WebviewWindow("debug-canvas", {
        url: "index.html#debug-canvas",
        title: "调试画面",
        width: 1280,
        height: 720,
        resizable: true,
      });
      // Wait for the OS to finish creating the webview before
      // re-broadcasting state. We deliberately do NOT register
      // `onCloseRequested` here — that handler intercepts the
      // native close action and forces us to call `destroy()`,
      // which then needs window-close IPC perms and is easy to
      // get wrong. `tauri://destroyed` fires after the window
      // has actually closed, which is all we need to flip the
      // button label back.
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

  const imageSrc = capture
    ? `${convertFileSrc(capture.imagePath)}?t=${cacheBuster}`
    : null;

  // -------------------------------------------------------------------
  // Popout window (DebugCanvasWindow) state sync
  // -------------------------------------------------------------------
  // The popout is a separate WebviewWindow rendering only the canvas,
  // so it can be dragged onto a second monitor without the main page's
  // toolbars and side-panels stealing space. State flows in one
  // direction:
  //
  //   DebugPage ── debug-canvas:state ──▶ DebugCanvasWindow
  //   DebugPage ◀── debug-canvas:request ── DebugCanvasWindow  (on mount)
  //
  // The popout has no debug state of its own, so on mount it emits a
  // `request` event; the host responds by re-emitting the current
  // snapshot. After that, every re-render of the host pushes the
  // snapshot out — emit is cheap and the popout is the only listener.
  const canvasState: DebugCanvasState = useMemo(
    () => ({
      imageSrc,
      probes,
      commandCards,
      noblePhantasms,
      battleScene,
      attackButton,
      supportResult,
      coordinates,
      showCoordOverlay,
      visibleCoordGroups: Array.from(visibleCoordGroups),
    }),
    [
      imageSrc,
      probes,
      commandCards,
      noblePhantasms,
      battleScene,
      attackButton,
      supportResult,
      coordinates,
      showCoordOverlay,
      visibleCoordGroups,
    ]
  );

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

  // If the popout already exists when the page mounts (e.g. user
  // navigated away from CV Debug and came back), reflect that in the
  // button label and watch for native close via `tauri://destroyed`.
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

            <button
              className="battle-btn battle-btn-start debug-btn-small"
              onClick={handleOpenPopout}
              title="把画面 + 标注弹到独立窗口，可拖到副屏全屏查看"
            >
              <ExternalLinkIcon width={12} height={12} />
              <Text size="1">{popoutOpen ? "聚焦画面窗口" : "弹出画面"}</Text>
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
                supportResult === null &&
                battleScene === null &&
                attackButton === null
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
            title="攻击按钮识别"
            badge={
              attackButton
                ? `${(attackButton.score * 100).toFixed(0)}% ${attackButton.found ? "✓" : "✗"}`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Text size="2" color="gray">
                复用 runner 的 `button_attack` 探针，确认战斗回合开始时是否能命中攻击按钮。
              </Text>
              <button
                className="battle-btn battle-btn-start debug-btn-small"
                disabled={findingAttackButton || !capture}
                onClick={handleFindAttackButton}
              >
                {findingAttackButton ? "识别中…" : "识别攻击按钮"}
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
            title="攻击按钮识别"
            badge={
              attackButton
                ? `${(attackButton.score * 100).toFixed(0)}% ${attackButton.found ? "✓" : "✗"}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!attackButton && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {attackButton && (
                <Box
                  className={`debug-match-entry ${attackButton.found ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      {attackButton.template}
                    </Text>
                    <Text
                      size="1"
                      color={attackButton.found ? "green" : "red"}
                    >
                      {attackButton.found ? "命中" : "未命中"}
                    </Text>
                  </Flex>
                  <Text size="1" color="gray">
                    score {attackButton.score.toFixed(3)} · 阈值{" "}
                    {attackButton.threshold.toFixed(2)}
                  </Text>
                  <Text size="1" color="gray">
                    搜索区域 (
                    {attackButton.region.x.toFixed(3)},{" "}
                    {attackButton.region.y.toFixed(3)},{" "}
                    {attackButton.region.w.toFixed(3)},{" "}
                    {attackButton.region.h.toFixed(3)})
                  </Text>
                  {attackButton.found && (
                    <Text size="1" color="gray">
                      命中中心 ({attackButton.matchX.toFixed(3)},{" "}
                      {attackButton.matchY.toFixed(3)})
                    </Text>
                  )}
                  <Text size="1" color="gray">
                    点击 ({attackButton.tapPoint.x.toFixed(3)},{" "}
                    {attackButton.tapPoint.y.toFixed(3)})
                  </Text>
                </Box>
              )}
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
