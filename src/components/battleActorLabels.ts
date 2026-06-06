import type { Servant } from "../types/servant";

export type BattleActorKind = "servant" | "equipment" | "commandSpell";

export function servantLabel(index: number, servant: Servant | null): string {
  return servant?.name_cn || `从者 ${index + 1}`;
}

export function battleActorLabel({
  kind,
  servant,
  index,
}: {
  kind: BattleActorKind;
  servant?: Servant | null;
  index?: number;
}): string {
  if (kind === "servant") {
    return servantLabel(index ?? 0, servant ?? null);
  }
  return kind === "equipment" ? "御主礼装" : "令咒";
}
