import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import {
  Box,
  Button,
  Checkbox,
  Flex,
  Select,
  Text,
  TextField,
} from "@radix-ui/themes";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  ExternalLinkIcon,
  ReloadIcon,
} from "@radix-ui/react-icons";
import { emit, invoke, listen, convertFileSrc } from "../tauri";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalSize } from "@tauri-apps/api/dpi";
import type { CvConfig } from "../types/cv";
import type { CraftEssence } from "../types/craftEssence";
import type { Servant } from "../types/servant";
import type { SupportGrandBondCeMode } from "../types/project";
import type { DebugCanvasState } from "./DebugCanvas";
import {
  DEBUG_CANVAS_REQUEST_EVENT,
  DEBUG_CANVAS_STATE_EVENT,
} from "./DebugCanvasWindow";
import { CraftEssenceSelectDialog } from "./CraftEssenceSelectDialog";
import { ServantSelectDialog } from "./ServantSelectDialog";

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
  critDigitRegions?: NormRectDto[];
  critDigitReads?: CritDigitReadDto[];
  suit?: "a" | "b" | "q";
  iconScore?: number;
  iconRegion?: NormRectDto;
  servantId?: number;
  ascension?: number;
  faceScore?: number;
  critChance?: number;
}

export interface CritDigitReadDto {
  digit: number | null;
  score: number;
  kept: boolean;
}

export interface NoblePhantasmMatchDto {
  slot: number;
  cardRegion: NormRectDto;
  ready: boolean;
  edgeFrac: number;
  stdBgr: number;
  edgeThreshold?: number;
}

export interface EnhancementServantFaceMatchDto {
  template: string;
  templatePath: string;
  row: number | null;
  col: number | null;
  found: boolean;
  score: number;
  x: number;
  y: number;
  region: NormRectDto | null;
  error?: string;
}

export interface EnhancementServantAnchorDto {
  x: number;
  y: number;
  w: number;
  h: number;
  score: number;
  edgeScore: number;
  grayScore: number;
  source: string;
  row?: number | null;
  col?: number | null;
}

export interface EnhancementServantGridCellDto {
  row: number;
  col: number;
  region: NormRectDto;
}

export interface EnhancementServantDiagnosticsDto {
  failReason?: string | null;
  anchorTemplateKey: string;
  anchorEdgeThreshold: number;
  anchorGrayThreshold: number;
  faceThreshold: number;
  region?: NormRectDto | null;
  anchorCount: number;
  gridCellCount: number;
  attempts: number;
}

export interface EnhancementServantMatchResultDto {
  servantId: number;
  searchRegion: NormRectDto;
  templateCrop: NormRectDto;
  templateSize: { w: number; h: number };
  threshold: number;
  found: boolean;
  x: number;
  y: number;
  score: number;
  best: EnhancementServantFaceMatchDto | null;
  anchors: EnhancementServantAnchorDto[];
  referenceAnchor: EnhancementServantAnchorDto | null;
  gridCells: EnhancementServantGridCellDto[];
  matches: EnhancementServantFaceMatchDto[];
  diagnostics: EnhancementServantDiagnosticsDto;
}

export interface DigitMatchDto {
  value: number;
  score: number;
  region: NormRectDto;
}

/**
 * Snapshot of the runner's attack-button probe (template
 * `Battle.variants.main.elements.attack_button` from cv.json). Surfaces both cv.json
 * template settings and the live match score so the user can tell why automation is stuck on
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

const EMPTY_DEBUG_SELECT_VALUE = "__empty__";

export interface SupportCeInfoDto {
  region: NormRectDto;
  score: number;
  passed: boolean;
  threshold: number;
  iconChecks?: SupportCeIconCheckDto[];
  templatePath?: string;
  error?: string;
}

export interface SupportCeIconCheckDto {
  kind: string;
  templateKey: string;
  region: NormRectDto;
  score: number;
  passed: boolean;
  threshold: number;
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
  scoreAnchor?: NormRectDto | null;
  npMatchedName: string;
  npLevel?: number | null;
  skillPanel?: "owned" | "append" | null;
  skillLevels?: (number | null)[];
  appendSkillLevels?: (number | null)[];
  skillLevelDiagnostics?: Array<{
    level?: number | null;
    score?: number;
    source?: string;
    region?: NormRectDto;
  }>;
  /**
   * Per-row craft-essence verification, populated only when the user
   * supplies a CE id in the debug toolbar. Lets the overlay draw the
   * CE search window and the score so `SUPPORT_CE_OFFSET_IN_ROW` /
   * `SUPPORT_CE_THRESHOLD` can be calibrated against real captures.
   */
  ce?: SupportCeInfoDto;
  grandCes?: SupportCeInfoDto[];
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
  cvFile?: string;
  cvFingerprint?: string;
  supportSkillContourSplit?: boolean;
  supportRowAnchorSearchRegion?: NormRectDto | null;
  confirmButtonAnchors?: NormRectDto[];
  isGrandSectionVisible?: boolean | null;
}

export interface FindSupportsResultDto {
  supports: SupportRowMatchDto[];
  diagnostics: SupportDiagnosticsDto;
}

function supportPanelLabel(panel: SupportRowMatchDto["skillPanel"]): string {
  if (panel === "owned") return "持有技能";
  if (panel === "append") return "追加技能";
  return "技能面板未知";
}

function supportLevelList(levels: (number | null)[] | undefined): string {
  if (!levels || levels.length === 0) return "未识别";
  return levels.map((level) => (level == null ? "-" : String(level))).join("/");
}

function supportSkillDiagnosticsText(
  diagnostics: SupportRowMatchDto["skillLevelDiagnostics"]
): string {
  if (!diagnostics || diagnostics.length === 0) return "";
  return diagnostics
    .map((item, index) => {
      const level = item.level == null ? "-" : String(item.level);
      const score =
        typeof item.score === "number" ? item.score.toFixed(2) : "0.00";
      return `${index + 1}:${level}@${score}${item.source ? `/${item.source}` : ""}`;
    })
    .join(" ");
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
  servants: Servant[];
  craftEssences: CraftEssence[];
  /**
   * Front-line servant ids (party slots 0–2 + support, deduped) derived
   * from the currently active project. Used to seed the "候选从者 id"
   * input so the face-matcher has candidates to compare against on first
   * click — without this, the input is empty and every card returns as
   * "未识别从者", which is what tripped users up before. Pass `[]` when
   * no project is active or none of the slots have a servant pinned.
   */
  defaultCardServantIds: number[];
}

type DebugServantPickerTarget = "card" | "support" | "enhancement";
const DEBUG_CANVAS_POPOUT_WIDTH = 1280;
const DEBUG_CANVAS_POPOUT_HEIGHT = 900;
const DEBUG_PREFS_STORAGE_KEY = "mash.debugPagePrefs.v1";

interface DebugPagePrefs {
  selectedScreen?: string;
  selectedElement?: string;
  rawTemplateInput?: string;
  threshold?: number;
  selectedCardServantIds?: number[];
  supportServantId?: number | null;
  supportCraftEssenceId?: number | null;
  supportCraftEssenceMlbRequired?: boolean;
  supportGrandCraftEssenceIds?: [number | null, number | null, number | null];
  supportGrandCraftEssenceMlbRequired?: [boolean, boolean, boolean];
  supportGrandBondCeMode?: SupportGrandBondCeMode;
  enhancementServantId?: number | null;
  enhancementServantThreshold?: string;
  showCoordOverlay?: boolean;
  visibleCoordGroups?: string[];
}

function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

function readDebugPrefs(): DebugPagePrefs {
  try {
    const raw = window.localStorage.getItem(DEBUG_PREFS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as DebugPagePrefs;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeDebugPrefs(prefs: DebugPagePrefs) {
  try {
    window.localStorage.setItem(DEBUG_PREFS_STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // Debug preferences are best-effort; private mode/quota errors should
    // never break the page.
  }
}

function asNumberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function asGrandCeIds(value: unknown): [number | null, number | null, number | null] {
  if (!Array.isArray(value)) return [null, null, null];
  return [
    asNumberOrNull(value[0]),
    asNumberOrNull(value[1]),
    asNumberOrNull(value[2]),
  ];
}

function asGrandMlbRequired(value: unknown): [boolean, boolean, boolean] {
  if (!Array.isArray(value)) return [true, true, true];
  return [value[0] !== false, value[1] !== false, value[2] !== false];
}

export function DebugPage({
  onBack,
  servants,
  craftEssences,
  defaultCardServantIds,
}: DebugPageProps) {
  const initialPrefs = useMemo(() => readDebugPrefs(), []);
  const [capture, setCapture] = useState<DebugCaptureResult | null>(null);
  const [cacheBuster, setCacheBuster] = useState(0);

  const [cvConfig, setCvConfig] = useState<CvConfig | null>(null);
  const [templateKeys, setTemplateKeys] = useState<string[]>([]);
  const [selectedScreen, setSelectedScreen] = useState<string>(
    initialPrefs.selectedScreen ?? ""
  );
  const [selectedElement, setSelectedElement] = useState<string>(
    initialPrefs.selectedElement ?? ""
  );
  const [rawTemplateInput, setRawTemplateInput] = useState(
    initialPrefs.rawTemplateInput ?? ""
  );
  const [threshold, setThreshold] = useState(
    typeof initialPrefs.threshold === "number" &&
      Number.isFinite(initialPrefs.threshold)
      ? Math.max(0, Math.min(1, initialPrefs.threshold))
      : 0.8
  );

  const [probes, setProbes] = useState<ProbeResult[]>([]);
  const [capturing, setCapturing] = useState(false);
  const [probing, setProbing] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const [coordinates, setCoordinates] = useState<RunnerCoordinatesDto | null>(
    null
  );
  const [showCoordOverlay, setShowCoordOverlay] = useState(
    initialPrefs.showCoordOverlay === true
  );
  const [visibleCoordGroups, setVisibleCoordGroups] = useState<Set<string>>(
    () => new Set(initialPrefs.visibleCoordGroups ?? [])
  );

  const [availableServantIds, setAvailableServantIds] = useState<number[]>([]);
  // Seed the candidate list with the active project's front-line team.
  // The lazy initializer runs once on mount; users can still add/remove
  // names afterwards, and switching projects remounts the page via the
  // view router so we'll re-seed naturally.
  const [selectedCardServantIds, setSelectedCardServantIds] = useState<
    number[]
  >(() =>
    Array.isArray(initialPrefs.selectedCardServantIds)
      ? initialPrefs.selectedCardServantIds.filter(
          (id): id is number => typeof id === "number" && Number.isFinite(id)
        )
      : defaultCardServantIds
  );
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
  const [supportServantId, setSupportServantId] = useState<number | null>(
    asNumberOrNull(initialPrefs.supportServantId)
  );
  const [supportCraftEssenceId, setSupportCraftEssenceId] =
    useState<number | null>(asNumberOrNull(initialPrefs.supportCraftEssenceId));
  const [supportCraftEssenceMlbRequired, setSupportCraftEssenceMlbRequired] =
    useState(initialPrefs.supportCraftEssenceMlbRequired !== false);
  const [supportGrandCraftEssenceIds, setSupportGrandCraftEssenceIds] =
    useState<[number | null, number | null, number | null]>(() =>
      asGrandCeIds(initialPrefs.supportGrandCraftEssenceIds)
    );
  const [
    supportGrandCraftEssenceMlbRequired,
    setSupportGrandCraftEssenceMlbRequired,
  ] = useState<[boolean, boolean, boolean]>(() =>
    asGrandMlbRequired(initialPrefs.supportGrandCraftEssenceMlbRequired)
  );
  const [supportGrandBondCeMode, setSupportGrandBondCeMode] =
    useState<SupportGrandBondCeMode>(initialPrefs.supportGrandBondCeMode ?? "any");
  const [supportMetadata, setSupportMetadata] =
    useState<ServantMetadataDto | null>(null);
  const [supportResult, setSupportResult] =
    useState<FindSupportsResultDto | null>(null);
  const [findingSupports, setFindingSupports] = useState(false);
  const [enhancementServantId, setEnhancementServantId] =
    useState<number | null>(asNumberOrNull(initialPrefs.enhancementServantId));
  const [enhancementServantThreshold, setEnhancementServantThreshold] =
    useState(initialPrefs.enhancementServantThreshold ?? "0.85");
  const [enhancementServantResult, setEnhancementServantResult] =
    useState<EnhancementServantMatchResultDto | null>(null);
  const [findingEnhancementServant, setFindingEnhancementServant] =
    useState(false);
  const [servantPickerTarget, setServantPickerTarget] =
    useState<DebugServantPickerTarget | null>(null);
  const [craftEssencePickerOpen, setCraftEssencePickerOpen] = useState(false);
  const [craftEssencePickerTarget, setCraftEssencePickerTarget] = useState<
    "single" | 0 | 1 | 2 | null
  >(null);
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
      setSelectedScreen((current) =>
        screenNames.length > 0 && !screenNames.includes(current)
          ? screenNames[0]
          : current
      );
    } catch (err) {
      log(`读取 cv.json 失败: ${err}`, "error");
    }
  }, [log]);

  const loadTemplateList = useCallback(async () => {
    try {
      const keys = await invoke<string[]>("debug_list_templates");
      if (!Array.isArray(keys)) {
        throw new Error("debug_list_templates 未返回数组");
      }
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
      if (!Array.isArray(ids)) {
        throw new Error("debug_list_servant_assets 未返回数组");
      }
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

  const visibleCoordGroupIds = useMemo(
    () => Array.from(visibleCoordGroups),
    [visibleCoordGroups]
  );

  useEffect(() => {
    writeDebugPrefs({
      selectedScreen,
      selectedElement,
      rawTemplateInput,
      threshold,
      selectedCardServantIds,
      supportServantId,
      supportCraftEssenceId,
      supportCraftEssenceMlbRequired,
      supportGrandCraftEssenceIds,
      supportGrandCraftEssenceMlbRequired,
      supportGrandBondCeMode,
      enhancementServantId,
      enhancementServantThreshold,
      showCoordOverlay,
      visibleCoordGroups: visibleCoordGroupIds,
    });
  }, [
    selectedScreen,
    selectedElement,
    rawTemplateInput,
    threshold,
    selectedCardServantIds,
    supportServantId,
    supportCraftEssenceId,
    supportCraftEssenceMlbRequired,
    supportGrandCraftEssenceIds,
    supportGrandCraftEssenceMlbRequired,
    supportGrandBondCeMode,
    enhancementServantId,
    enhancementServantThreshold,
    showCoordOverlay,
    visibleCoordGroupIds,
  ]);

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

  const collectElementNames = useCallback((screenName: string) => {
    if (!cvConfig) return [];
    const screen = cvConfig.screens[screenName];
    if (!screen) return [];

    const names: string[] = [];
    if (screen.detect) {
      names.push("detect");
    }
    if (screen.elements) {
      names.push(...Object.keys(screen.elements));
    }
    for (const [variantName, variant] of Object.entries(screen.variants || {})) {
      const prefix = `variants.${variantName}`;
      if (variant.detect) {
        names.push(`${prefix}.detect`);
      }
      if (variant.elements) {
        names.push(
          ...Object.keys(variant.elements).map((element) => `${prefix}.elements.${element}`)
        );
      }
    }
    return names;
  }, [cvConfig]);

  const resolveElementSpec = useCallback((screenName: string, elementName: string) => {
    if (!cvConfig) return null;
    const screen = cvConfig.screens[screenName];
    if (!screen) return null;
    if (elementName === "detect") return screen.detect ?? null;
    if (screen.elements?.[elementName]) return screen.elements[elementName];

    const match = /^variants\.([^.]+)\.(.+)$/.exec(elementName);
    if (!match) return null;
    const [, variantName, variantElement] = match;
    const variant = screen.variants?.[variantName];
    if (!variant) return null;
    if (variantElement === "detect") return variant.detect ?? null;
    if (variantElement.startsWith("elements.")) {
      return variant.elements?.[variantElement.slice("elements.".length)] ?? null;
    }
    return variant.elements?.[variantElement] ?? null;
  }, [cvConfig]);

  // Auto-select the first element when screen changes
  const elementNames = useMemo(() => {
    if (!selectedScreen) return [];
    return collectElementNames(selectedScreen);
  }, [collectElementNames, selectedScreen]);

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
    return resolveElementSpec(selectedScreen, selectedElement);
  }, [cvConfig, resolveElementSpec, selectedScreen, selectedElement]);

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
      setEnhancementServantResult(null);
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
    setEnhancementServantResult(null);
    log("已清除标注");
  }, [log]);

  const servantById = useMemo(() => {
    const map = new Map<number, Servant>();
    for (const servant of servants) {
      if (!map.has(servant.id)) {
        map.set(servant.id, servant);
      }
    }
    return map;
  }, [servants]);

  const availableServantIdSet = useMemo(
    () => new Set(availableServantIds),
    [availableServantIds]
  );

  const availableServants = useMemo(() => {
    if (availableServantIdSet.size === 0) return servants;
    return servants.filter((servant) => availableServantIdSet.has(servant.id));
  }, [availableServantIdSet, servants]);

  const selectedSupportServant = supportServantId
    ? servantById.get(supportServantId) ?? null
    : null;
  const selectedEnhancementServant = enhancementServantId
    ? servantById.get(enhancementServantId) ?? null
    : null;
  const selectedSupportCraftEssence = supportCraftEssenceId
    ? craftEssences.find((ce) => ce.id === supportCraftEssenceId) ?? null
    : null;
  const selectedSupportGrandCraftEssences = supportGrandCraftEssenceIds.map((id) =>
    id == null ? null : craftEssences.find((ce) => ce.id === id) ?? null
  );

  const displayServantName = useCallback(
    (id: number) => {
      const servant = servantById.get(id);
      if (!servant) return `#${id}`;
      return servant.name_cn_server?.trim() || servant.name_cn;
    },
    [servantById]
  );

  const handleSelectDebugServant = useCallback(
    (servant: Servant) => {
      if (servantPickerTarget === "card") {
        setSelectedCardServantIds((prev) =>
          prev.includes(servant.id) ? prev : [...prev, servant.id]
        );
      } else if (servantPickerTarget === "support") {
        setSupportServantId(servant.id);
        setSupportMetadata(null);
        setSupportResult(null);
      } else if (servantPickerTarget === "enhancement") {
        setEnhancementServantId(servant.id);
        setEnhancementServantResult(null);
      }
      setServantPickerTarget(null);
    },
    [servantPickerTarget]
  );

  const handleRemoveCardServant = useCallback((id: number) => {
    setSelectedCardServantIds((prev) => prev.filter((item) => item !== id));
  }, []);

  const handleFindCommandCards = useCallback(async () => {
    if (!capture) return;
    setFindingCards(true);
    log(
      `调用 debug_find_command_cards (servants=[${selectedCardServantIds.map(displayServantName).join(", ")}])`
    );
    try {
      const cards = await invoke<CommandCardMatchDto[]>(
        "debug_find_command_cards",
        { servantIds: selectedCardServantIds }
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
  }, [capture, selectedCardServantIds, displayServantName, log]);

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
    setSelectedCardServantIds(availableServantIds);
  }, [availableServantIds]);

  const parsedEnhancementServantThreshold = useMemo<number>(() => {
    const n = Number(enhancementServantThreshold.trim());
    return Number.isFinite(n) && n > 0 && n <= 1 ? n : 0.72;
  }, [enhancementServantThreshold]);

  const handleFindEnhancementServant = useCallback(async () => {
    if (!capture || enhancementServantId === null) return;
    setFindingEnhancementServant(true);
    log(
      `调用 debug_find_enhancement_servant (servant=${displayServantName(enhancementServantId)}, threshold=${parsedEnhancementServantThreshold})`
    );
    try {
      const result = await invoke<EnhancementServantMatchResultDto>(
        "debug_find_enhancement_servant",
        {
          servantId: enhancementServantId,
          threshold: parsedEnhancementServantThreshold,
        }
      );
      setEnhancementServantResult(result);
      const sorted = [...result.matches].sort((a, b) => b.score - a.score);
      const best = sorted[0];
      log(
        `强化从者网格: anchors=${result.anchors.length}, cells=${result.gridCells.length}, attempts=${result.diagnostics.attempts || 1}`
      );
      if (!best) {
        log(
          `从者 #${result.servantId}: 没有可用头像模板 (${result.diagnostics.failReason ?? "unknown"})`,
          "warn"
        );
      } else if (best.found) {
        log(
          `强化从者 #${result.servantId}: 最佳 r${best.row}c${best.col} ${best.template} score=${best.score.toFixed(3)} ✓ center=(${best.x.toFixed(3)}, ${best.y.toFixed(3)})`
        );
      } else {
        log(
          `强化从者 #${result.servantId}: 最佳 r${best.row}c${best.col} ${best.template} score=${best.score.toFixed(3)} < ${result.threshold.toFixed(2)} (${result.diagnostics.failReason ?? "unknown"})`,
          "warn"
        );
      }
      for (const m of sorted.slice(0, 6)) {
        log(
          `  r${m.row}c${m.col} ${m.template}: ${m.score.toFixed(3)} ${m.found ? "✓" : "✗"}` +
            (m.region ? ` @ (${m.x.toFixed(3)}, ${m.y.toFixed(3)})` : "")
        );
      }
    } catch (err) {
      log(`debug_find_enhancement_servant 失败: ${err}`, "error");
    } finally {
      setFindingEnhancementServant(false);
    }
  }, [
    capture,
    enhancementServantId,
    parsedEnhancementServantThreshold,
    displayServantName,
    log,
  ]);

  const handleFindSupports = useCallback(async () => {
    if (!capture || supportServantId === null) return;
    setFindingSupports(true);
    log(`调用 debug_find_supports (servant=${displayServantName(supportServantId)})`);
    try {
      // Fetch metadata first so the log + side panel can show what we
      // actually fed the OCR detector (helpful when a row misses).
      let meta = supportMetadata;
      if (!meta || meta.id !== supportServantId) {
        meta = await invoke<ServantMetadataDto>("get_servant_metadata", {
          id: supportServantId,
        });
        setSupportMetadata(meta);
        log(
          `加载从者元数据: ${meta.name} · 宝具 [${meta.npNames.join(" / ")}]`
        );
      }
      const result = await invoke<FindSupportsResultDto>(
        "debug_find_supports",
        {
          servantId: supportServantId,
          craftEssenceId: supportCraftEssenceId,
          grandCraftEssenceIds: supportGrandCraftEssenceIds,
          craftEssenceMlbRequired: supportCraftEssenceMlbRequired,
          grandCraftEssenceMlbRequired: supportGrandCraftEssenceMlbRequired,
          grandBondCeMode: supportGrandBondCeMode,
        }
      );
      setSupportResult(result);
      const diag = result.diagnostics;
      const cvInfo =
        diag.cvFingerprint || diag.cvFile
          ? ` · CV ${diag.cvFingerprint || "unknown"} split:${
              diag.supportSkillContourSplit ? "on" : "off"
            }`
          : "";
      log(
        `识别到 ${result.supports.length} 行助战 | OCR ${diag.fragmentCount} 片段` +
          ` · 名称候选 ${diag.nameCandidates.length}` +
          ` · 宝具候选 ${diag.npCandidates.length}` +
          cvInfo
      );
      // Surface the "冠位从者" ribbon probe — info level on a hit
      // so it lines up with the runner's user-facing operation log,
      // muted-gray otherwise. `null`/`undefined` means the active
      // server bundle didn't ship the template (e.g. JP), so we
      // skip the line instead of pretending we know.
      if (diag.isGrandSectionVisible === true) {
        log(
          `识别到冠位从者：助战编队确认按钮 ${
            (diag.confirmButtonAnchors ?? []).length
          } 个，其中至少一行命中冠位 ROI`
        );
      } else if (diag.isGrandSectionVisible === false) {
        log(
          `未识别到冠位从者（${
            (diag.confirmButtonAnchors ?? []).length
          } 个助战编队确认按钮均未命中冠位 ROI）`
        );
      }
      if (diag.nameOnlyFallback) {
        // Surface the degraded path immediately instead of waiting for
        // the user to expand the diagnostics panel — when this fires
        // the rows are real but their CE / NP cross-check was skipped.
        const why =
          diag.nameOnlyReason === "noNpExpected"
            ? "目标宝具没有可用的 CN OCR 名称映射"
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
          const npLevelPart =
            s.npLevel != null ? ` | 宝具等级 ${s.npLevel}` : "";
          const skillLevels =
            s.skillPanel === "append"
              ? supportLevelList(s.appendSkillLevels)
              : supportLevelList(s.skillLevels);
          const skillPart = s.skillPanel
            ? ` | ${supportPanelLabel(s.skillPanel)} ${skillLevels}`
            : "";
          const skillDiag = supportSkillDiagnosticsText(s.skillLevelDiagnostics);
          const skillDiagPart = skillDiag ? ` | 技能诊断 ${skillDiag}` : "";
          const scoreAnchorPart = s.scoreAnchor
            ? ` | 确认锚点 (${s.scoreAnchor.x.toFixed(3)}, ${s.scoreAnchor.y.toFixed(3)})`
            : " | 确认锚点未识别";
          const iconCheckPart = [
            ...(s.ce?.iconChecks ?? []),
            ...(s.grandCes ?? []).flatMap((ce) => ce.iconChecks ?? []),
          ]
            .map(
              (check) =>
                `${check.kind} ${check.score.toFixed(2)}/${check.threshold.toFixed(2)} ${check.passed ? "✓" : "✗"}`
            )
            .join(" · ");
          log(
            `  行 y=${s.rowRegion.y.toFixed(3)} | 名称='${s.nameText}' (${s.nameScore.toFixed(2)})` +
              npPart +
              npLevelPart +
              skillPart +
              skillDiagPart +
              scoreAnchorPart +
              cePart +
              (iconCheckPart ? ` | 图标 ${iconCheckPart}` : "")
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
    supportServantId,
    supportCraftEssenceId,
    supportGrandCraftEssenceIds,
    supportCraftEssenceMlbRequired,
    supportGrandCraftEssenceMlbRequired,
    supportGrandBondCeMode,
    supportMetadata,
    displayServantName,
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
    log("重启 sidecar 并重新加载模板/配置…");
    try {
      await invoke("debug_reload_sidecar");
      log("sidecar 已重启，模板、cv.json 与 CV 代码已重新读取");
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
      enhancementServantResult,
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
      enhancementServantResult,
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
          <Button type="button" variant="ghost" color="gray" onClick={onBack}>
            <ChevronLeftIcon width={16} height={16} />
            <Text size="2">返回</Text>
          </Button>
          <Text size="4" weight="bold">
            CV 调试
          </Text>
        </Flex>
        <Button
          type="button"
          size="2"
          variant="surface"
          color="gray"
          disabled={reloading}
          onClick={handleReloadSidecar}
          title="重启 sidecar，并重新加载 CV 代码、cv.json 和模板"
        >
          <ReloadIcon width={14} height={14} />
          <Text size="1">{reloading ? "重启中…" : "重启 CV/重载配置"}</Text>
        </Button>
      </Flex>

      <Flex className="debug-body" gap="4">
        <Flex direction="column" className="debug-canvas-col" gap="2">
          <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
            <Button
              type="button"
              size="3"
              disabled={capturing}
              onClick={handleCapture}
            >
              {capturing ? "截取中…" : "截取画面"}
            </Button>

            <Button
              type="button"
              size="1"
              variant="surface"
              onClick={handleOpenPopout}
              title="把画面 + 标注弹到独立窗口，可拖到副屏全屏查看"
            >
              <ExternalLinkIcon width={12} height={12} />
              <Text size="1">{popoutOpen ? "聚焦画面窗口" : "弹出画面"}</Text>
            </Button>

            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                画面
              </Text>
              <Select.Root
                value={selectedScreen || EMPTY_DEBUG_SELECT_VALUE}
                onValueChange={(value) => setSelectedScreen(value)}
                disabled={screenNames.length === 0}
              >
                <Select.Trigger className="debug-select-trigger" aria-label="画面" />
                <Select.Content>
                  {screenNames.length === 0 && (
                    <Select.Item value={EMPTY_DEBUG_SELECT_VALUE} disabled>
                      —
                    </Select.Item>
                  )}
                  {screenNames.map((s) => (
                    <Select.Item key={s} value={s}>
                      {s}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Flex>

            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                元素
              </Text>
              <Select.Root
                value={selectedElement || EMPTY_DEBUG_SELECT_VALUE}
                onValueChange={(value) => setSelectedElement(value)}
                disabled={elementNames.length === 0}
              >
                <Select.Trigger className="debug-select-trigger" aria-label="元素" />
                <Select.Content>
                  {elementNames.length === 0 && (
                    <Select.Item value={EMPTY_DEBUG_SELECT_VALUE} disabled>
                      —
                    </Select.Item>
                  )}
                  {elementNames.map((e) => (
                    <Select.Item key={e} value={e}>
                      {e}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Flex>

            <Button
              type="button"
              disabled={
                probing || !capture || !selectedScreen || !selectedElement
              }
              onClick={handleProbeByName}
            >
              {probing ? "查找中…" : "查找元素"}
            </Button>

            <Button
              type="button"
              color="red"
              disabled={
                probes.length === 0 &&
                commandCards.length === 0 &&
                noblePhantasms.length === 0 &&
                supportResult === null &&
                enhancementServantResult === null &&
                battleScene === null &&
                attackButton === null
              }
              onClick={handleClearOverlays}
            >
              清除标注
            </Button>
          </Flex>

          <DebugSection title="原始模板探针">
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <TextField.Root
                className="debug-text-field"
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
                <TextField.Root
                  className="debug-threshold-field"
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
              <Button
                type="button"
                size="1"
                disabled={probing || !capture || !rawTemplateInput.trim()}
                onClick={handleProbeRaw}
              >
                {probing ? "查找中…" : "查找"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection title="指令卡 / 宝具卡识别">
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Flex align="center" gap="1" wrap="wrap" className="debug-servant-choice-list">
                {selectedCardServantIds.length === 0 ? (
                  <Text size="1" color="gray">
                    未选择候选从者，可留空只定位卡槽
                  </Text>
                ) : (
                  selectedCardServantIds.map((id) => (
                    <Button
                      key={`card-servant-${id}`}
                      type="button"
                      size="1"
                      variant="soft"
                      color="gray"
                      onClick={() => handleRemoveCardServant(id)}
                      title="点击移除"
                    >
                      {displayServantName(id)}
                    </Button>
                  ))
                )}
              </Flex>
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setServantPickerTarget("card")}
              >
                添加从者
              </Button>
              <Button
                type="button"
                size="1"
                variant="surface"
                disabled={availableServantIds.length === 0}
                onClick={handleUseAllAvailableIds}
                title={`填入全部 ${availableServantIds.length} 个有素材的从者 id`}
              >
                全选 ({availableServantIds.length})
              </Button>
              <Button
                type="button"
                size="1"
                disabled={findingCards || !capture}
                onClick={handleFindCommandCards}
              >
                {findingCards ? "识别中…" : "识别指令卡"}
              </Button>
              <Button
                type="button"
                size="1"
                disabled={findingNps || !capture}
                onClick={handleFindNoblePhantasms}
              >
                {findingNps ? "识别中…" : "识别宝具卡"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="强化从者头像识别"
            badge={
              enhancementServantResult
                ? `#${enhancementServantResult.servantId}`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setServantPickerTarget("enhancement")}
              >
                {selectedEnhancementServant
                  ? displayServantName(selectedEnhancementServant.id)
                  : "选择从者"}
              </Button>
              <TextField.Root
                className="debug-threshold-field"
                type="number"
                step="0.01"
                min="0"
                max="1"
                value={enhancementServantThreshold}
                onChange={(e) => setEnhancementServantThreshold(e.target.value)}
                title="强化从者头像匹配阈值"
              />
              <Button
                type="button"
                size="1"
                disabled={
                  findingEnhancementServant ||
                  !capture ||
                  enhancementServantId === null
                }
                onClick={handleFindEnhancementServant}
              >
                {findingEnhancementServant ? "识别中…" : "识别强化从者"}
              </Button>
              <Text size="2" color="gray">
                使用生产逻辑的从者列表区域和 face crop。
              </Text>
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
              <Button
                type="button"
                size="1"
                disabled={readingBattleScene || !capture}
                onClick={handleReadBattleScene}
              >
                {readingBattleScene ? "识别中…" : "识别战斗场景"}
              </Button>
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
                复用 `cv.json` 的 `Battle.variants.main.elements.attack_button` 探针，确认战斗回合开始时是否能命中攻击按钮。
              </Text>
              <Button
                type="button"
                size="1"
                disabled={findingAttackButton || !capture}
                onClick={handleFindAttackButton}
              >
                {findingAttackButton ? "识别中…" : "识别攻击按钮"}
              </Button>
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
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setServantPickerTarget("support")}
              >
                {selectedSupportServant
                  ? displayServantName(selectedSupportServant.id)
                  : "选择助战从者"}
              </Button>
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => {
                  setCraftEssencePickerTarget("single");
                  setCraftEssencePickerOpen(true);
                }}
                title="留空跳过礼装识别。选择后会按行运行 verify_support_ce 并叠加搜索框 + 分数。"
              >
                {selectedSupportCraftEssence
                  ? selectedSupportCraftEssence.name
                  : "选择礼装 (可选)"}
              </Button>
              {supportCraftEssenceId !== null && (
                <Button
                  type="button"
                  size="1"
                  variant="ghost"
                  color="gray"
                  onClick={() => {
                    setSupportCraftEssenceId(null);
                    setSupportCraftEssenceMlbRequired(true);
                  }}
                >
                  清除礼装
                </Button>
              )}
              <label className="debug-coord-toggle">
                <Checkbox
                  checked={supportCraftEssenceMlbRequired}
                  onCheckedChange={(checked) =>
                    setSupportCraftEssenceMlbRequired(checked === true)
                  }
                />
                <Text size="1">满破</Text>
              </label>
              {[0, 1, 2].map((index) => {
                const ce = selectedSupportGrandCraftEssences[index];
                return (
                  <Flex key={index} align="center" gap="1">
                    <Button
                      type="button"
                      size="1"
                      variant="surface"
                      onClick={() => {
                        setCraftEssencePickerTarget(index as 0 | 1 | 2);
                        setCraftEssencePickerOpen(true);
                      }}
                      title="冠位战右侧三张礼装按位置匹配；留空的位置不校验。"
                    >
                      {ce ? `冠${index + 1}: ${ce.name}` : `冠位礼装 ${index + 1}`}
                    </Button>
                    <label className="debug-coord-toggle">
                      <Checkbox
                        checked={supportGrandCraftEssenceMlbRequired[index]}
                        onCheckedChange={(checked) => {
                          setSupportGrandCraftEssenceMlbRequired((prev) => {
                            const next = [...prev] as [boolean, boolean, boolean];
                            next[index] = checked === true;
                            return next;
                          });
                        }}
                      />
                      <Text size="1">满破</Text>
                    </label>
                  </Flex>
                );
              })}
              <Flex gap="1" align="center">
                <Text size="1" color="gray">
                  冠2牵绊
                </Text>
                <Select.Root
                  value={supportGrandBondCeMode}
                  onValueChange={(value) =>
                    setSupportGrandBondCeMode(value as SupportGrandBondCeMode)
                  }
                >
                  <Select.Trigger aria-label="Debug 冠位第二礼装牵绊形态" />
                  <Select.Content>
                    <Select.Item value="any">任意</Select.Item>
                    <Select.Item value="bond">原始</Select.Item>
                    <Select.Item value="bondNp">连接</Select.Item>
                  </Select.Content>
                </Select.Root>
              </Flex>
              {supportGrandCraftEssenceIds.some((id) => id != null) && (
                <Button
                  type="button"
                  size="1"
                  variant="ghost"
                  color="gray"
                  onClick={() => {
                    setSupportGrandCraftEssenceIds([null, null, null]);
                    setSupportGrandCraftEssenceMlbRequired([true, true, true]);
                    setSupportGrandBondCeMode("any");
                  }}
                >
                  清除冠位礼装
                </Button>
              )}
              <Button
                type="button"
                size="1"
                disabled={
                  findingSupports || !capture || supportServantId === null
                }
                onClick={handleFindSupports}
              >
                {findingSupports ? "识别中…" : "识别助战"}
              </Button>
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
                <Checkbox
                  checked={showCoordOverlay}
                  onCheckedChange={(checked) =>
                    setShowCoordOverlay(checked === true)
                  }
                />
                <Text size="1">坐标叠层</Text>
              </label>
              {coordinates?.groups.map((g) => (
                <label key={g.id} className="debug-coord-toggle">
                  <Checkbox
                    disabled={!showCoordOverlay}
                    checked={visibleCoordGroups.has(g.id)}
                    onCheckedChange={(checked) => {
                      setVisibleCoordGroups((prev) => {
                        const next = new Set(prev);
                        if (checked === true) next.add(g.id);
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
              <Button
                type="button"
                size="1"
                variant="surface"
                color="gray"
                disabled={logs.length === 0}
                onClick={handleClearLogs}
              >
                清空
              </Button>
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
                  模板:{" "}
                  {selectedElementSpec.templates?.join(", ") ??
                    selectedElementSpec.template ??
                    "(未配置)"}
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
                    {c.critChance !== undefined
                      ? ` · 暴击 ${c.critChance}%`
                      : ""}
                  </Text>
                  {c.critDigitReads && c.critDigitReads.length > 0 && (
                    <Text size="1" color="gray">
                      暴击位{" "}
                      {c.critDigitReads
                        .map((r) => {
                          const glyph =
                            r.digit !== null && r.digit !== undefined
                              ? String(r.digit)
                              : "-";
                          const score = r.score.toFixed(2);
                          return r.kept
                            ? `${glyph}@${score}`
                            : `(${glyph}@${score})`;
                        })
                        .join(" · ")}
                    </Text>
                  )}
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
            title="强化从者头像识别"
            badge={
              enhancementServantResult
                ? `${enhancementServantResult.matches.filter((m) => m.found).length}/${enhancementServantResult.matches.length}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!enhancementServantResult && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {enhancementServantResult && (
                <Box className="debug-match-entry">
                  <Text size="2" weight="medium">
                    目标：#{enhancementServantResult.servantId}
                  </Text>
                  <Text size="1" color="gray">
                    模板缩放: {enhancementServantResult.templateSize.w} ×{" "}
                    {enhancementServantResult.templateSize.h}
                  </Text>
                  <Text size="1" color="gray">
                    crop (
                    {enhancementServantResult.templateCrop.x.toFixed(3)},{" "}
                    {enhancementServantResult.templateCrop.y.toFixed(3)},{" "}
                    {enhancementServantResult.templateCrop.w.toFixed(3)},{" "}
                    {enhancementServantResult.templateCrop.h.toFixed(3)})
                  </Text>
                  <Text size="1" color="gray">
                    搜索区域 (
                    {enhancementServantResult.searchRegion.x.toFixed(3)},{" "}
                    {enhancementServantResult.searchRegion.y.toFixed(3)},{" "}
                    {enhancementServantResult.searchRegion.w.toFixed(3)},{" "}
                    {enhancementServantResult.searchRegion.h.toFixed(3)})
                  </Text>
                  <Text size="1" color="gray">
                    anchors {enhancementServantResult.anchors.length} · cells{" "}
                    {enhancementServantResult.gridCells.length} · attempts{" "}
                    {enhancementServantResult.diagnostics.attempts || 1}
                    {enhancementServantResult.diagnostics.failReason
                      ? ` · ${enhancementServantResult.diagnostics.failReason}`
                      : ""}
                  </Text>
                </Box>
              )}
              {enhancementServantResult?.referenceAnchor && (
                <Box className="debug-match-entry found">
                  <Text size="2" weight="medium">
                    参考 anchor
                  </Text>
                  <Text size="1" color="gray">
                    col {enhancementServantResult.referenceAnchor.col ?? "?"} ·
                    edge{" "}
                    {enhancementServantResult.referenceAnchor.edgeScore.toFixed(3)} ·
                    gray{" "}
                    {enhancementServantResult.referenceAnchor.grayScore.toFixed(3)}
                  </Text>
                </Box>
              )}
              {enhancementServantResult?.matches
                .slice()
                .sort((a, b) => b.score - a.score)
                .map((m) => (
                  <Box
                    key={`enhancement-side-${m.row}-${m.col}-${m.template}`}
                    className={`debug-match-entry ${m.found ? "found" : "missed"}`}
                  >
                    <Flex justify="between" align="center">
                      <Text size="2" weight="medium">
                        {m.template}
                      </Text>
                      <Text size="1" color={m.found ? "green" : "red"}>
                        {m.score.toFixed(3)}
                      </Text>
                    </Flex>
                    <Text size="1" color="gray">
                      r{m.row} c{m.col} · 阈值{" "}
                      {enhancementServantResult.threshold.toFixed(2)}
                      {m.region
                        ? ` · center (${m.x.toFixed(3)}, ${m.y.toFixed(3)})`
                        : ""}
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
                  {supportResult.diagnostics.isGrandSectionVisible != null && (
                    <Text
                      size="1"
                      color={
                        supportResult.diagnostics.isGrandSectionVisible
                          ? "amber"
                          : "gray"
                      }
                    >
                      冠位段：
                      {supportResult.diagnostics.isGrandSectionVisible
                        ? "✓ 已识别"
                        : "✗ 未识别"}
                      （{(supportResult.diagnostics.confirmButtonAnchors ?? []).length}{" "}
                      个 ROI 探测）
                    </Text>
                  )}
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
                    {s.npLevel != null ? ` · 等级 ${s.npLevel}` : ""}
                  </Text>
                  {s.skillPanel && (
                    <Text size="1" color="blue">
                      {supportPanelLabel(s.skillPanel)}：
                      {s.skillPanel === "append"
                        ? supportLevelList(s.appendSkillLevels)
                        : supportLevelList(s.skillLevels)}
                    </Text>
                  )}
                  {s.skillLevelDiagnostics && s.skillLevelDiagnostics.length > 0 && (
                    <Text size="1" color="gray">
                      技能诊断：{supportSkillDiagnosticsText(s.skillLevelDiagnostics)}
                    </Text>
                  )}
                  <Text size="1" color="gray">
                    行 y={s.rowRegion.y.toFixed(3)} h=
                    {s.rowRegion.h.toFixed(3)}
                  </Text>
                  <Text size="1" color={s.scoreAnchor ? "green" : "amber"}>
                    确认锚点：
                    {s.scoreAnchor
                      ? `(${s.scoreAnchor.x.toFixed(3)}, ${s.scoreAnchor.y.toFixed(3)}, ${s.scoreAnchor.w.toFixed(3)}, ${s.scoreAnchor.h.toFixed(3)})`
                      : "未识别"}
                  </Text>
                  {s.ce && (
                    <>
                      <Text
                        size="1"
                        color={s.ce.passed ? "green" : "red"}
                      >
                        礼装 {s.ce.score.toFixed(3)} /{" "}
                        {s.ce.threshold.toFixed(2)}{" "}
                        {s.ce.passed ? "✓ 匹配" : "✗ 未达阈值"}
                        {s.ce.error ? ` · ${s.ce.error}` : ""}
                      </Text>
                      {(s.ce.iconChecks ?? []).map((check) => (
                        <Text
                          key={`ce-icon-${check.kind}-${check.templateKey}`}
                          size="1"
                          color={check.passed ? "green" : "red"}
                        >
                          图标 {check.kind}: {check.score.toFixed(3)} /{" "}
                          {check.threshold.toFixed(2)}{" "}
                          {check.passed ? "✓" : "✗"}
                          {check.error ? ` · ${check.error}` : ""}
                        </Text>
                      ))}
                    </>
                  )}
                  {(s.grandCes ?? []).map((ce, ceIndex) => (
                    <Text
                      key={`grand-ce-side-${i}-${ceIndex}`}
                      size="1"
                      color={ce.passed ? "green" : "red"}
                    >
                      冠{ceIndex + 1} {ce.score.toFixed(3)} /{" "}
                      {ce.threshold.toFixed(2)}{" "}
                      {ce.passed ? "✓" : "✗"}
                      {(ce.iconChecks ?? [])
                        .map(
                          (check) =>
                            ` · ${check.kind} ${check.score.toFixed(2)}${check.passed ? "✓" : "✗"}`
                        )
                        .join("")}
                    </Text>
                  ))}
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
      <ServantSelectDialog
        open={servantPickerTarget !== null}
        onOpenChange={(open) => {
          if (!open) setServantPickerTarget(null);
        }}
        onSelect={handleSelectDebugServant}
        servants={availableServants}
        disabledIds={
          servantPickerTarget === "card" ? selectedCardServantIds : undefined
        }
      />
      <CraftEssenceSelectDialog
        open={craftEssencePickerOpen}
        onOpenChange={(open) => {
          setCraftEssencePickerOpen(open);
          if (!open) setCraftEssencePickerTarget(null);
        }}
        onSelect={(ce) => {
          if (craftEssencePickerTarget === "single") {
            setSupportCraftEssenceId(ce.id);
          } else if (craftEssencePickerTarget != null) {
            setSupportGrandCraftEssenceIds((prev) => {
              const next = [...prev] as [number | null, number | null, number | null];
              next[craftEssencePickerTarget] = ce.id;
              return next;
            });
          }
          setSupportResult(null);
        }}
        craftEssences={craftEssences}
        mlbRequired={
          craftEssencePickerTarget === "single"
            ? supportCraftEssenceMlbRequired
            : craftEssencePickerTarget != null
              ? supportGrandCraftEssenceMlbRequired[craftEssencePickerTarget]
              : true
        }
        onMlbRequiredChange={
          craftEssencePickerTarget === "single"
            ? setSupportCraftEssenceMlbRequired
            : craftEssencePickerTarget != null
              ? (required) =>
                  setSupportGrandCraftEssenceMlbRequired((prev) => {
                    const next = [...prev] as [boolean, boolean, boolean];
                    next[craftEssencePickerTarget] = required;
                    return next;
                  })
              : undefined
        }
        grandBondCeMode={supportGrandBondCeMode}
        onGrandBondCeModeChange={
          craftEssencePickerTarget === 1 ? setSupportGrandBondCeMode : undefined
        }
      />
    </Flex>
  );
}
