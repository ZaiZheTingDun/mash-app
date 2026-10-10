import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "../../tauri";
import type { CommandSpell } from "../../types/command";
import type { SkillEntry } from "../team/useServantSkillIcons";

export type CommandSpellSkillIcons = Record<CommandSpell, SkillEntry>;
const EMPTY_ICONS: CommandSpellSkillIcons = {
  np_release: { src: null, name: "宝具解放" },
  restore: { src: null, name: "灵基修复" },
};

export function useCommandSpellSkillIcons() {
  const [icons, setIcons] = useState(EMPTY_ICONS);
  useEffect(() => {
    let cancelled = false;
    invoke<[{ path: string | null; name: string }, { path: string | null; name: string }]>("get_command_spell_icon_paths")
      .then(entries => {
        if (cancelled) return;
        const entry = (index: number, spell: CommandSpell): SkillEntry => {
          const resource = entries[index];
          return {
            src: resource?.path ? convertFileSrc(resource.path) : null,
            name: resource?.name || EMPTY_ICONS[spell].name,
          };
        };
        setIcons({ np_release: entry(0, "np_release"), restore: entry(1, "restore") });
      })
      .catch(() => { if (!cancelled) setIcons(EMPTY_ICONS); });
    return () => { cancelled = true; };
  }, []);
  return icons;
}
