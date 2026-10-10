import { BattleActorIcon } from "./BattleActorIcon";
import { masterFacePath, type MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";
import { convertFileSrc } from "../../tauri";

export function EnemyTargetActionSummary({ target, code, gender = "female", label }: {
  target: string | null;
  code?: MysticCode | null;
  gender?: MysticCodeGender;
  label: string;
}) {
  const path = code ? masterFacePath(code, gender) : null;
  return <span className="battle-action-summary" aria-label={label}>
    <span className="command-row-source">
      <BattleActorIcon kind="equipment" src={path ? convertFileSrc(path) : null} label="御主" size="inline" />
      <span className="battle-action-name">御主</span>
    </span>
    <span className="command-row-skill"><span className="command-row-skill-name" title="选择敌方目标">选择敌方目标</span></span>
    <span className="command-row-outcome">{target ? target.replace("enemy_", "").padStart(2, "0") : "?"}</span>
  </span>;
}
