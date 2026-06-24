import { SKILLS, SKILL_LABELS } from "../../features/battle/battleSceneModel";
import type { SkillIcons } from "../../features/team/useServantSkillIcons";
import type { Servant } from "../../types/servant";

interface SkillOptionButtonsProps {
  servant: Servant | null;
  skillIcons: Record<string, SkillIcons>;
  onSelect: (skill: string) => void;
}

export function SkillOptionButtons({ servant, skillIcons, onSelect }: SkillOptionButtonsProps) {
  return SKILLS.map((skill, skillIndex) => {
    const iconSrc = servant
      ? (skillIcons[servant.variantKey]?.[skillIndex] ?? null)
      : null;
    return (
      <button
        type="button"
        key={skill}
        className={`battle-option-btn${iconSrc ? " skill-icon" : ""}`}
        onClick={() => onSelect(skill)}
      >
        {iconSrc
          ? <img src={iconSrc} alt={SKILL_LABELS[skill]} draggable={false} />
          : SKILL_LABELS[skill]}
      </button>
    );
  });
}
