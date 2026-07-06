import { useEffect, useMemo, useState } from "react";
import { invoke } from "../../tauri";
import { skillSlotIndex } from "./battleSceneModel";
import type { SkillSelectionType } from "../../types/command";
import type { Servant } from "../../types/servant";

export interface ServantSkillSelectionOption {
  index: number;
  label: string;
}

export interface ServantSkillSelectionEntry {
  servantCollectionNo: number;
  skillId: number;
  skillNum: number;
  selectionType: SkillSelectionType;
  supplementaryTypes: SkillSelectionType[];
  options: ServantSkillSelectionOption[];
}

type SelectionByVariant = Record<string, Map<number, ServantSkillSelectionEntry> | null>;

export function useServantSkillSelections(servants: (Servant | null)[]) {
  const [selections, setSelections] = useState<SelectionByVariant>({});

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
    const missing = parsed.filter(({ variantKey }) => !(variantKey in selections));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<ServantSkillSelectionEntry[]>("get_servant_skill_selection", {
          servantId: request.servantId,
          variantKey: request.variantKey,
        })
          .then((entries) => {
            const bySkillNum = new Map<number, ServantSkillSelectionEntry>();
            for (const entry of entries) {
              if (entry.skillNum >= 1 && entry.skillNum <= 3) {
                bySkillNum.set(entry.skillNum, entry);
              }
            }
            return [request.variantKey, bySkillNum] as const;
          })
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setSelections((prev) => {
        const next = { ...prev };
        for (const [variantKey, bySkillNum] of results) next[variantKey] = bySkillNum;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestKey]);

  return (
    servant: Servant | null,
    skill: string | null | undefined
  ): ServantSkillSelectionEntry | null => {
    if (!servant) return null;
    const skillIndex = skillSlotIndex(skill);
    if (skillIndex < 0) return null;
    const bySkillNum = selections[servant.variantKey];
    if (!bySkillNum) return null;
    return bySkillNum.get(skillIndex + 1) ?? null;
  };
}
