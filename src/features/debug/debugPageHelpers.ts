import type { SupportGrandBondCeMode } from "../../types/project";
import type {
  SupportCeArtworkCheckDto,
  SupportCeIconCheckDto,
  SupportRowMatchDto,
} from "./debugTypes";

export const BATTLE_SCENE_FAIL_HINTS: Record<string, string> = {
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

export const EMPTY_DEBUG_SELECT_VALUE = "__empty__";

const DEBUG_PREFS_STORAGE_KEY = "mash.debugPagePrefs.v1";

export interface DebugPagePrefs {
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

export function supportPanelLabel(panel: SupportRowMatchDto["skillPanel"]): string {
  if (panel === "owned") return "持有技能";
  if (panel === "append") return "追加技能";
  return "技能面板未知";
}

export function supportLevelList(levels: (number | null)[] | undefined): string {
  if (!levels || levels.length === 0) return "未识别";
  return levels.map((level) => (level == null ? "-" : String(level))).join("/");
}

export function supportSkillDiagnosticsText(
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

export function supportCeArtworkChecksText(
  checks: SupportCeArtworkCheckDto[] | undefined
): string {
  if (!checks || checks.length === 0) return "";
  const selectedRegionKind = checks.find((check) => check.selected)?.regionKind;
  return checks
    .filter((check) => !selectedRegionKind || check.regionKind === selectedRegionKind)
    .map((check) => {
      const marker = check.selected ? "*" : check.passed ? "✓" : "✗";
      return `${supportCeArtworkVariantLabel(check.variant)}（${check.score.toFixed(2)}/${check.threshold.toFixed(2)}）${marker}`;
    })
    .join(" · ");
}

export function supportCeIconChecksText(
  checks: SupportCeIconCheckDto[] | undefined
): string {
  if (!checks || checks.length === 0) return "";
  return checks
    .map(
      (check) =>
        `${supportCeIconKindLabel(check.kind)}（${check.score.toFixed(2)}/${check.threshold.toFixed(2)}）${check.passed ? "✓" : "✗"}`
    )
    .join(" · ");
}

function supportCeArtworkVariantLabel(variant: string): string {
  if (variant === "full") return "完整匹配";
  if (variant === "center") return "中间匹配";
  if (variant === "top_right") return "右上匹配";
  return variant;
}

function supportCeIconKindLabel(kind: string): string {
  if (kind === "mlb") return "满破图标";
  if (kind === "grandBond" || kind === "grandBondNp") return "牵绊图标";
  return kind;
}

export function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

export function readDebugPrefs(): DebugPagePrefs {
  try {
    const raw = window.localStorage.getItem(DEBUG_PREFS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as DebugPagePrefs;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function writeDebugPrefs(prefs: DebugPagePrefs) {
  try {
    window.localStorage.setItem(DEBUG_PREFS_STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // Debug preferences are best-effort; private mode/quota errors should
    // never break the page.
  }
}

export function asNumberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function asGrandCeIds(value: unknown): [number | null, number | null, number | null] {
  if (!Array.isArray(value)) return [null, null, null];
  return [
    asNumberOrNull(value[0]),
    asNumberOrNull(value[1]),
    asNumberOrNull(value[2]),
  ];
}

export function asGrandMlbRequired(value: unknown): [boolean, boolean, boolean] {
  if (!Array.isArray(value)) return [true, true, true];
  return [value[0] !== false, value[1] !== false, value[2] !== false];
}
