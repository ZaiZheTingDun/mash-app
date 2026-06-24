import { useEffect, useMemo, useState } from "react";
import { convertFileSrc, invoke } from "../../tauri";
import type { Servant } from "../../types/servant";

export type SkillEntry = { src: string | null; name: string };
export type SkillIcons = [SkillEntry, SkillEntry, SkillEntry];

export function useServantSkillIcons(servants: (Servant | null)[]) {
  const [skillIcons, setSkillIcons] = useState<Record<string, SkillIcons>>({});

  const requests = useMemo(() => {
    const seen = new Set<string>();
    return servants
      .filter((servant): servant is Servant => Boolean(servant))
      .filter((servant) => {
        if (seen.has(servant.variantKey)) return false;
        seen.add(servant.variantKey);
        return true;
      })
      .map((servant) => ({
        variantKey: servant.variantKey,
        servantId: servant.id,
      }))
      .sort((a, b) => a.variantKey.localeCompare(b.variantKey));
  }, [servants]);
  const requestKey = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(requestKey) as typeof requests;
    const missing = parsed.filter(({ variantKey }) => !(variantKey in skillIcons));
    if (missing.length === 0) return;
    let cancelled = false;
    const emptyIcons = (): SkillIcons => [
      { src: null, name: "" },
      { src: null, name: "" },
      { src: null, name: "" },
    ];
    Promise.all(
      missing.map((request) =>
        invoke<[{ path: string | null; name: string }, { path: string | null; name: string }, { path: string | null; name: string }]>("get_skill_icon_paths", {
          servantId: request.servantId,
          variantKey: request.variantKey,
        })
          .then((entries) => {
            const icons: SkillIcons = [
              { src: entries[0].path ? convertFileSrc(entries[0].path) : null, name: entries[0].name },
              { src: entries[1].path ? convertFileSrc(entries[1].path) : null, name: entries[1].name },
              { src: entries[2].path ? convertFileSrc(entries[2].path) : null, name: entries[2].name },
            ];
            return [request.variantKey, icons] as const;
          })
          .catch(() => [request.variantKey, emptyIcons()] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setSkillIcons((prev) => {
        const next = { ...prev };
        for (const [variantKey, icons] of results) next[variantKey] = icons;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestKey]);

  return skillIcons;
}
