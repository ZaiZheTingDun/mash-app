import { Avatar } from "@radix-ui/themes";
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
    const entry = servant ? (skillIcons[servant.variantKey]?.[skillIndex] ?? null) : null;
    const iconSrc = entry?.src ?? null;
    const label = entry?.name || SKILL_LABELS[skill];
    return (
      <button
        type="button"
        key={skill}
        className="battle-option-btn skill-icon"
        title={label}
        onClick={() => onSelect(skill)}
      >
        <Avatar
          src={iconSrc ?? undefined}
          fallback={String(skillIndex + 1)}
          alt={label}
          radius="small"
          size="3"
        />
      </button>
    );
  });
}
