import type {
  GrandClass,
  Project,
  SupportAppendSkillLevelMins,
  SupportGrandCraftEssenceIds,
  SupportGrandCraftEssenceMlbRequired,
  SupportSkillLevelMins,
} from "../../types/project";

export const EMPTY_SUPPORT_SKILL_LEVELS: SupportSkillLevelMins = [null, null, null];
export const EMPTY_SUPPORT_APPEND_SKILL_LEVELS: SupportAppendSkillLevelMins = [
  null,
  null,
  null,
  null,
  null,
];

export function normalizeSupportSkillLevels(
  levels: Project["supportSkillLevelMins"],
): SupportSkillLevelMins {
  return [0, 1, 2].map((index) => levels?.[index] ?? null) as SupportSkillLevelMins;
}

export function normalizeSupportAppendSkillLevels(
  levels: Project["supportAppendSkillLevelMins"],
): SupportAppendSkillLevelMins {
  return [0, 1, 2, 3, 4].map((index) => levels?.[index] ?? null) as SupportAppendSkillLevelMins;
}

export function hasConfiguredLevels(levels: readonly (number | null | undefined)[]) {
  return levels.some((level) => level != null);
}

export function normalizeSupportGrandCraftEssenceIds(
  ids: Project["supportGrandCraftEssenceIds"],
): SupportGrandCraftEssenceIds {
  return [0, 1, 2].map((index) => ids?.[index] ?? null) as SupportGrandCraftEssenceIds;
}

export function normalizeSupportGrandCraftEssenceMlbRequired(
  values: Project["supportGrandCraftEssenceMlbRequired"],
): SupportGrandCraftEssenceMlbRequired {
  return [0, 1, 2].map((index) => values?.[index] ?? true) as SupportGrandCraftEssenceMlbRequired;
}

export function grandClassToServantClass(grandClass: GrandClass | undefined): string {
  return grandClass === "berserker" ? "Berserker" : "Saber";
}
