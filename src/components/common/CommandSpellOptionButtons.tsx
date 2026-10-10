import { Avatar } from "@radix-ui/themes";
import { useCommandSpellSkillIcons } from "../../features/battle/useCommandSpellSkillIcons";
import type { CommandSpell } from "../../types/command";

export function CommandSpellOptionButtons({ onSelect }: { onSelect: (spell: string) => void }) {
  const icons = useCommandSpellSkillIcons();
  return (["np_release", "restore"] as CommandSpell[]).map(spell => {
    const { src, name: label } = icons[spell];
    return <button type="button" key={spell} className="battle-option-btn skill-icon command-skill-option"
      aria-label={label} title={label} onClick={() => onSelect(spell)}>
      <Avatar src={src ?? undefined} alt={label} fallback={label.slice(0, 2)} radius="small" size="3" />
      <span className="command-skill-name">{label}</span>
    </button>;
  });
}
