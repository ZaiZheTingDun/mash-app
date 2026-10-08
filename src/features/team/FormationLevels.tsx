import { Button } from "@radix-ui/themes";
import { MixerHorizontalIcon } from "@radix-ui/react-icons";
import { hasConfiguredLevels } from "./supportSettingsModel";
import type { SupportSkillLevelMins, SupportAppendSkillLevelMins } from "../../types/project";

/** Render the saved filter summary, or its settings entry when no filters are active. */
export function FormationLevels({support = false, level, np, skills = [null, null, null], append = [null, null, null, null, null], starMapScore, grandStarMapScore, onOpen}: {
  onOpen: () => void;
  support?: boolean; level?: number | null; np?: number | null;
  starMapScore?: number | null; grandStarMapScore?: number | null;
  skills?: SupportSkillLevelMins; append?: SupportAppendSkillLevelMins;
}) {
  const row = (values: readonly (number | null)[], kind: "skill" | "append", label: string) => values.map((value, index) =>
    <span key={index} className={`support-requirement-chip ${kind}${value == null ? " unset" : ""}`} aria-label={`${label} ${index + 1} ${value == null ? support ? "任意等级" : "未记录等级" : `至少 ${value} 级`}`}>{value ?? "—"}</span>
  );
  const showStats = level != null || np != null;
  const showScores = starMapScore != null || grandStarMapScore != null;
  const showSkills = hasConfiguredLevels(skills);
  const showAppend = hasConfiguredLevels(append);
  if (!showStats && !showScores && !showSkills && !showAppend) {
    return <Button type="button" variant="soft" className="support-filter-entry" onClick={onOpen}>
      <MixerHorizontalIcon width={12} height={12} />
      助战筛选设置
    </Button>;
  }

  return (
    <button type="button" className="formation-levels support-requirement-summary" aria-label="编辑助战筛选设置" onClick={onOpen}>
      {showStats && <span className="formation-levels-stats">
        {level != null && <span className="support-requirement-chip score servant-level" aria-label={`从者至少 ${level} 级`}>Lv.{level}</span>}
        {np != null && <span className="support-requirement-chip np" aria-label={`宝具至少 ${np} 级`}>宝具 {np}</span>}
      </span>}
      {showScores && <span className="formation-levels-scores support-requirement-score-row">
        {starMapScore != null && <span className="support-requirement-chip score" aria-label={`星图至少 ${starMapScore}`}>星图 {starMapScore}</span>}
        {grandStarMapScore != null && <span className="support-requirement-chip score grand-score" aria-label={`冠位至少 ${grandStarMapScore}`}>冠位 {grandStarMapScore}</span>}
      </span>}
      {showSkills && <span className="formation-levels-skills"><small>SKILL</small>{row(skills,"skill","持有技能")}</span>}
      {showAppend && <span className="formation-levels-append">{row(append,"append","追加技能")}</span>}
    </button>
  );
}
