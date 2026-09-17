import type {
  BattleApRecoveryItem,
  BattleApRecoveryLimits,
  GrandChainPriorityItem,
  Project,
} from "../../types/project";

const AP_RECOVERY_ORDER: BattleApRecoveryItem[] = [
  "gold",
  "silver",
  "bronze",
  "copper",
  "rainbow",
];
const MAX_AP_RECOVERY_LIMIT = 4_294_967_295;
const DEFAULT_GRAND_CHAIN_PRIORITY: GrandChainPriorityItem[] = [
  "mainBraveChain",
  "mainReadyNp",
  "deputyBraveChain",
  "mainColorChain",
  "deputyColorChain",
  "fallback",
];
const DEFAULT_FIVE_STAR_CE_DROP_TARGET_COUNT = 1;

function orderedApRecoveryItems(project: Project): BattleApRecoveryItem[] {
  const configured = project.apRecoveryItems ?? [];
  return AP_RECOVERY_ORDER.filter((value) => configured.includes(value));
}

function projectApRecoveryLimits(project: Project): BattleApRecoveryLimits {
  const configured = project.apRecoveryLimits;
  return AP_RECOVERY_ORDER.reduce<BattleApRecoveryLimits>(
    (limits, item) => {
      const value = configured?.[item];
      limits[item] =
        typeof value === "number" && Number.isInteger(value) && value > 0
          ? Math.min(value, MAX_AP_RECOVERY_LIMIT)
          : null;
      return limits;
    },
    { rainbow: null, gold: null, silver: null, bronze: null, copper: null },
  );
}

export interface ProjectRunConfigOptions {
  repeatMission?: boolean;
  maxMissionRuns?: number | null;
  apRecoveryItems?: BattleApRecoveryItem[];
  apRecoveryLimits?: BattleApRecoveryLimits;
}

export function buildProjectRunConfig(
  project: Project,
  options: ProjectRunConfigOptions = {},
) {
  const supportSlot = project.slots?.find((slot) => slot.type === "support");
  const servantSelections =
    project.slots
      ?.map((slot, slotIndex) =>
        slot.type === "servant" && slot.servantId != null
          ? { memberId: slot.id, slotIndex, servantId: slot.servantId }
          : null
      )
      .filter(
        (selection): selection is {
          memberId: string;
          slotIndex: number;
          servantId: number;
        } => selection != null,
      ) ?? [];
  const stopOnFiveStarCeDrop =
    project.recognitionSettings?.stopOnFiveStarCeDrop === true;
  const configuredDropTarget = project.recognitionSettings?.fiveStarCeDropTargetCount;
  const fiveStarCeDropTargetCount =
    typeof configuredDropTarget === "number" &&
    Number.isInteger(configuredDropTarget) &&
    configuredDropTarget > 0
      ? configuredDropTarget
      : DEFAULT_FIVE_STAR_CE_DROP_TARGET_COUNT;

  return {
    projectId: project.id,
    mysticCodeId: project.mysticCodeId ?? null,
    partyOrder: null,
    supportClassFilter: null,
    supportServantName: null,
    supportServantId: project.supportServantId ?? null,
    supportServantVariantKey: project.supportServantVariantKey ?? null,
    supportSlotIndex: supportSlot != null ? project.slots?.indexOf(supportSlot) ?? null : null,
    supportMemberId: supportSlot?.id ?? null,
    supportCraftEssenceId: supportSlot?.craftEssenceId ?? null,
    supportCraftEssenceIds: supportSlot?.craftEssenceIds?.length
      ? supportSlot.craftEssenceIds.slice(0, 10)
      : supportSlot?.craftEssenceId != null
        ? [supportSlot.craftEssenceId]
        : [],
    supportCraftEssenceMlbRequired: supportSlot?.craftEssenceMlbRequired ?? true,
    supportGrandMode: project.supportGrandMode ?? false,
    supportGrandCraftEssenceIds: project.supportGrandCraftEssenceIds ?? [null, null, null],
    supportGrandCraftEssenceIdLists: project.supportGrandCraftEssenceIdLists ?? [[], [], []],
    supportGrandCraftEssenceMlbRequired:
      project.supportGrandCraftEssenceMlbRequired ?? [true, true, true],
    supportGrandBondCeMode: project.supportGrandBondCeMode ?? "any",
    grandServants: project.grandServants ?? [],
    grandClass: project.grandClass ?? "saber",
    grandCardStrategy: project.grandCardStrategy ?? {
      chainPriority: DEFAULT_GRAND_CHAIN_PRIORITY,
    },
    supportServantLevelMin: project.supportServantLevelMin ?? null,
    supportNoblePhantasmLevelMin: project.supportNoblePhantasmLevelMin ?? null,
    supportStarMapScoreMin: project.supportStarMapScoreMin ?? null,
    supportGrandStarMapScoreMin: project.supportGrandStarMapScoreMin ?? null,
    supportSkillLevelMins: project.supportSkillLevelMins ?? [null, null, null],
    supportAppendSkillLevelMins:
      project.supportAppendSkillLevelMins ?? [null, null, null, null, null],
    preferHigherCriticalChance: project.preferHigherCriticalChance ?? false,
    servantSelections,
    maxSupportScrolls: 3,
    repeatMission: options.repeatMission ?? false,
    maxMissionRuns: options.maxMissionRuns ?? null,
    apRecoveryItems: options.apRecoveryItems ?? orderedApRecoveryItems(project),
    apRecoveryLimits: options.apRecoveryLimits ?? projectApRecoveryLimits(project),
    stopOnFiveStarCeDrop,
    fiveStarCeDropTargetCount: stopOnFiveStarCeDrop
      ? fiveStarCeDropTargetCount
      : DEFAULT_FIVE_STAR_CE_DROP_TARGET_COUNT,
  };
}
