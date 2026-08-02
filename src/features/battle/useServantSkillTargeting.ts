import { useEffect, useMemo, useState } from "react";
import { invoke } from "../../tauri";
import { skillSlotIndex } from "./battleSceneModel";
import type { Servant } from "../../types/servant";

export type SkillTargetStatus = "needsTarget" | "noTarget" | "mixed" | "unknown";

export interface ServantSkillTargetingEntry {
  servantCollectionNo: number;
  skillId: number;
  skillNum: number;
  funcTargetTypes: ("ptOne" | "ptOneOther")[];
  targetingMode?: SkillTargetStatus;
}

type TargetingByVariant = Record<string, Map<number, SkillTargetStatus> | null>;

export function useServantSkillTargeting(servants: (Servant | null)[]) {
  const [targeting, setTargeting] = useState<TargetingByVariant>({});

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
    const missing = parsed.filter(({ variantKey }) => !(variantKey in targeting));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<ServantSkillTargetingEntry[]>("get_servant_skill_targeting", {
          servantId: request.servantId,
          variantKey: request.variantKey,
        })
          .then((entries) => {
            const statuses = new Map<number, SkillTargetStatus>([
              [1, "noTarget"],
              [2, "noTarget"],
              [3, "noTarget"],
            ]);
            for (const entry of entries) {
              if (entry.skillNum < 1 || entry.skillNum > 3) continue;
              statuses.set(
                entry.skillNum,
                entry.targetingMode ??
                  (entry.funcTargetTypes.length > 0 ? "needsTarget" : "noTarget")
              );
            }
            return [request.variantKey, statuses] as const;
          })
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setTargeting((prev) => {
        const next = { ...prev };
        for (const [variantKey, statuses] of results) next[variantKey] = statuses;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestKey]);

  return (servant: Servant | null, skill: string): SkillTargetStatus => {
    if (!servant) return "unknown";
    const skillIndex = skillSlotIndex(skill);
    if (skillIndex < 0) return "unknown";
    const statuses = targeting[servant.variantKey];
    if (statuses === undefined || statuses === null) return "unknown";
    return statuses.get(skillIndex + 1) ?? "noTarget";
  };
}
