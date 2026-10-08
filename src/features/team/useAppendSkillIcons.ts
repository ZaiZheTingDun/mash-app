import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "../../tauri";
import type { SkillEntry } from "./useServantSkillIcons";

type AppendSkillIcons = [SkillEntry, SkillEntry, SkillEntry, SkillEntry, SkillEntry];
type AppendSkillIconPaths = [
  { path: string | null; name: string },
  { path: string | null; name: string },
  { path: string | null; name: string },
  { path: string | null; name: string },
  { path: string | null; name: string },
];
const EMPTY_ICONS: AppendSkillIcons = Array.from({ length: 5 }, () => ({ src: null, name: "" })) as AppendSkillIcons;

export function useAppendSkillIcons() {
  const [icons, setIcons] = useState<AppendSkillIcons>(EMPTY_ICONS);

  useEffect(() => {
    let cancelled = false;
    invoke<AppendSkillIconPaths>("get_append_skill_icon_paths")
      .then(entries => {
        if (cancelled) return;
        setIcons(entries.map(entry => ({
          src: entry.path ? convertFileSrc(entry.path) : null,
          name: entry.name,
        })) as AppendSkillIcons);
      })
      .catch(() => {
        if (!cancelled) setIcons(EMPTY_ICONS);
      });
    return () => { cancelled = true; };
  }, []);

  return icons;
}
