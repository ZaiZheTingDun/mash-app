import type { MysticCodeGender } from "./appUiSettings";

export type MysticCodeSkillMode =
  | "needsTarget"
  | "noTarget"
  | "orderChange"
  | "unknown";

export interface MysticCodeSkill {
  id: number;
  slot: number;
  name: string;
  iconPath: string | null;
  targetingMode: MysticCodeSkillMode;
}

export interface MysticCode {
  id: number;
  name: string;
  itemMalePath: string | null;
  itemFemalePath: string | null;
  masterFigureMalePath: string | null;
  masterFigureFemalePath: string | null;
  masterFaceMalePath: string | null;
  masterFaceFemalePath: string | null;
  skills: MysticCodeSkill[];
}

export function masterFigurePath(code: MysticCode, gender: MysticCodeGender): string | null {
  return gender === "male" ? code.masterFigureMalePath : code.masterFigureFemalePath;
}

export function masterFacePath(code: MysticCode, gender: MysticCodeGender): string | null {
  return gender === "male" ? code.masterFaceMalePath : code.masterFaceFemalePath;
}

export function mysticCodeSkill(code: MysticCode | null | undefined, skill: string) {
  const slot = Number(skill.replace("skill_", ""));
  return Number.isInteger(slot) ? code?.skills.find((entry) => entry.slot === slot) ?? null : null;
}

export function mysticCodeItemPath(code: MysticCode, gender: MysticCodeGender): string | null {
  return gender === "male" ? code.itemMalePath : code.itemFemalePath;
}
