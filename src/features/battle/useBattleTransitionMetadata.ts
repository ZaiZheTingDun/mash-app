import { useEffect, useMemo, useState } from "react";
import { invoke } from "../../tauri";
import type {
  BattleTransitionPreviewEvent,
  ResolvedBattleMetadata,
} from "../../types/battleTransition";
import type { PartyMember } from "../team/partyServants";

export function battleMemberKey(member: PartyMember, index: number): string {
  return member.memberId ?? `${index}:${member.servant?.id ?? 0}:${member.isSupport ? 1 : 0}`;
}

export function useBattleTransitionMetadata(
  members: PartyMember[],
  eventsForMember: (member: PartyMember, index: number) => BattleTransitionPreviewEvent[]
) {
  const requests = useMemo(
    () =>
      members.map((member, index) => ({
        key: battleMemberKey(member, index),
        servantId: member.servant?.id ?? null,
        variantKey: member.servant?.variantKey ?? null,
        events: eventsForMember(member, index),
      })),
    [members, eventsForMember]
  );
  const requestKey = JSON.stringify(requests);
  const [metadata, setMetadata] = useState<Record<string, ResolvedBattleMetadata | null>>({});

  useEffect(() => {
    const parsed = JSON.parse(requestKey) as typeof requests;
    let cancelled = false;
    Promise.all(
      parsed.map(async (request) => {
        if (request.servantId == null || !request.variantKey) {
          return [request.key, null] as const;
        }
        try {
          const result = await invoke<ResolvedBattleMetadata | null>("resolve_battle_metadata", {
            servantId: request.servantId,
            variantKey: request.variantKey,
            events: request.events,
          });
          return [
            request.key,
            result && Array.isArray(result.skills) && Array.isArray(result.availableConditions)
              ? result
              : null,
          ] as const;
        } catch {
          return [request.key, null] as const;
        }
      })
    ).then((results) => {
      if (!cancelled) setMetadata(Object.fromEntries(results));
    });
    return () => {
      cancelled = true;
    };
  }, [requestKey]);

  return metadata;
}
