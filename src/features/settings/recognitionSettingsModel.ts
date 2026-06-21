import type { RecognitionSettings } from "../../types/recognition";

const SUPPORT_THRESHOLD_DEFAULT = 0.7;
const SUPPORT_THRESHOLD_MIN = 0.6;
const SUPPORT_THRESHOLD_MAX = 0.85;
const SUPPORT_FULL_GATE_THRESHOLD_DEFAULT = 0.6;
const SUPPORT_FULL_GATE_THRESHOLD_MIN = 0.4;
const SUPPORT_FULL_GATE_THRESHOLD_MAX = 0.7;

export const SUPPORT_THRESHOLD_STEP = 0.01;

export type ThresholdKey =
  | "supportCeThreshold"
  | "supportCeFullGateThreshold"
  | "supportMlbIconThreshold"
  | "supportBondIconThreshold";

export interface ThresholdConfig {
  key: ThresholdKey;
  label: string;
  description: (defaultValue: number) => string;
  ariaLabel: string;
  command: string;
  defaultValue: number;
  min: number;
  max: number;
}

export const THRESHOLD_CONFIGS: ThresholdConfig[] = [
  {
    key: "supportCeThreshold",
    label: "助战礼装匹配阈值",
    description: (defaultValue) =>
      `低数值更容易命中，高数值更不容易误选；默认 ${defaultValue.toFixed(2)}`,
    ariaLabel: "助战礼装匹配阈值数值",
    command: "set_support_ce_threshold",
    defaultValue: SUPPORT_THRESHOLD_DEFAULT,
    min: SUPPORT_THRESHOLD_MIN,
    max: SUPPORT_THRESHOLD_MAX,
  },
  {
    key: "supportCeFullGateThreshold",
    label: "完整匹配兜底阈值",
    description: (defaultValue) =>
      `中间或右上匹配命中时，完整匹配也必须至少达到该值；默认 ${defaultValue.toFixed(2)}`,
    ariaLabel: "完整匹配兜底阈值数值",
    command: "set_support_ce_full_gate_threshold",
    defaultValue: SUPPORT_FULL_GATE_THRESHOLD_DEFAULT,
    min: SUPPORT_FULL_GATE_THRESHOLD_MIN,
    max: SUPPORT_FULL_GATE_THRESHOLD_MAX,
  },
  {
    key: "supportMlbIconThreshold",
    label: "满破图标匹配阈值",
    description: (defaultValue) =>
      `用于匹配助战礼装满破图标；默认 ${defaultValue.toFixed(2)}`,
    ariaLabel: "满破图标匹配阈值数值",
    command: "set_support_mlb_icon_threshold",
    defaultValue: SUPPORT_THRESHOLD_DEFAULT,
    min: SUPPORT_THRESHOLD_MIN,
    max: SUPPORT_THRESHOLD_MAX,
  },
  {
    key: "supportBondIconThreshold",
    label: "牵绊图标匹配阈值",
    description: (defaultValue) =>
      `用于匹配冠位礼装牵绊 / 冠位连接牵绊图标；默认 ${defaultValue.toFixed(2)}`,
    ariaLabel: "牵绊图标匹配阈值数值",
    command: "set_support_bond_icon_threshold",
    defaultValue: SUPPORT_THRESHOLD_DEFAULT,
    min: SUPPORT_THRESHOLD_MIN,
    max: SUPPORT_THRESHOLD_MAX,
  },
];

export const DEFAULT_RECOGNITION_SETTINGS: RecognitionSettings = {
  supportCeThreshold: SUPPORT_THRESHOLD_DEFAULT,
  supportCeFullGateThreshold: SUPPORT_FULL_GATE_THRESHOLD_DEFAULT,
  supportMlbIconThreshold: SUPPORT_THRESHOLD_DEFAULT,
  supportBondIconThreshold: SUPPORT_THRESHOLD_DEFAULT,
};

export function clampThreshold(value: number, config: ThresholdConfig) {
  if (!Number.isFinite(value)) return config.defaultValue;
  return Math.min(config.max, Math.max(config.min, value));
}

export function normalizeRecognitionSettings(settings: RecognitionSettings): RecognitionSettings {
  return {
    supportCeThreshold: clampThreshold(settings.supportCeThreshold, THRESHOLD_CONFIGS[0]),
    supportCeFullGateThreshold: clampThreshold(
      settings.supportCeFullGateThreshold,
      THRESHOLD_CONFIGS[1]
    ),
    supportMlbIconThreshold: clampThreshold(
      settings.supportMlbIconThreshold,
      THRESHOLD_CONFIGS[2]
    ),
    supportBondIconThreshold: clampThreshold(
      settings.supportBondIconThreshold,
      THRESHOLD_CONFIGS[3]
    ),
  };
}

export function formatThreshold(value: number, config: ThresholdConfig) {
  return clampThreshold(value, config).toFixed(2);
}

export function settingsToDraft(settings: RecognitionSettings): Record<ThresholdKey, string> {
  return {
    supportCeThreshold: formatThreshold(settings.supportCeThreshold, THRESHOLD_CONFIGS[0]),
    supportCeFullGateThreshold: formatThreshold(
      settings.supportCeFullGateThreshold,
      THRESHOLD_CONFIGS[1]
    ),
    supportMlbIconThreshold: formatThreshold(
      settings.supportMlbIconThreshold,
      THRESHOLD_CONFIGS[2]
    ),
    supportBondIconThreshold: formatThreshold(
      settings.supportBondIconThreshold,
      THRESHOLD_CONFIGS[3]
    ),
  };
}
