import { useEffect, useMemo, useState } from "react";
import { convertFileSrc, invoke } from "../../tauri";
import type { Servant } from "../../types/servant";

export type SkillIcons = [string | null, string | null, string | null];

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
    Promise.all(
      missing.map((request) =>
        invoke<[string | null, string | null, string | null]>("get_skill_icon_paths", {
          servantId: request.servantId,
          variantKey: request.variantKey,
        })
          .then((paths) => {
            const icons: SkillIcons = [
              paths[0] ? convertFileSrc(paths[0]) : null,
              paths[1] ? convertFileSrc(paths[1]) : null,
              paths[2] ? convertFileSrc(paths[2]) : null,
            ];
            return [request.variantKey, icons] as const;
          })
          .catch(() => [request.variantKey, [null, null, null] as SkillIcons] as const)
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
