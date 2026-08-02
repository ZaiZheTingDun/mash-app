import { useState } from "react";
import { Button, Text } from "@radix-ui/themes";
import type { BattleStateOverride } from "../../types/command";
import type { ResolvedBattleMetadata } from "../../types/battleTransition";
import { servantLabel } from "../../components/common/battleActorLabels";
import type { PartyMember } from "../team/partyServants";
import { battleMemberKey } from "./useBattleTransitionMetadata";

interface BattleStateOverridesProps {
  members: PartyMember[];
  metadata: Record<string, ResolvedBattleMetadata | null>;
  value: BattleStateOverride[];
  onChange: (value: BattleStateOverride[]) => void;
}

function optionalPositiveInteger(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

export function BattleStateOverrides({
  members,
  metadata,
  value,
  onChange,
}: BattleStateOverridesProps) {
  const [expanded, setExpanded] = useState(false);
  const [remainingTurns, setRemainingTurns] = useState<Record<string, string>>({});
  const [stacks, setStacks] = useState<Record<string, string>>({});
  const relevant = members.flatMap((member, index) => {
    const memberKey = battleMemberKey(member, index);
    const resolved = metadata[memberKey];
    return (resolved?.availableConditions ?? []).map((condition) => ({
      member,
      index,
      memberKey,
      resolved,
      condition,
    }));
  });

  if (relevant.length === 0) return null;

  const updateOverride = (
    entry: (typeof relevant)[number],
    mode: BattleStateOverride["mode"]
  ) => {
    const key = `${entry.memberKey}:${entry.condition.stateKey}`;
    const next: BattleStateOverride = {
      memberId: entry.member.memberId,
      servantId: entry.member.servant?.id,
      isSupport: entry.member.isSupport,
      stateKey: entry.condition.stateKey,
      mode,
      ...(mode === "set"
        ? {
            remainingTurns: optionalPositiveInteger(remainingTurns[key] ?? ""),
            stacks: optionalPositiveInteger(stacks[key] ?? ""),
          }
        : {}),
    };
    onChange([
      ...value.filter(
        (override) =>
          !(
            override.stateKey === next.stateKey &&
            (next.memberId
              ? override.memberId === next.memberId
              : override.servantId === next.servantId &&
                Boolean(override.isSupport) === Boolean(next.isSupport))
          )
      ),
      next,
    ]);
  };

  return (
    <section className="battle-phase battle-state-overrides">
      <button
        type="button"
        className="battle-add-trigger"
        aria-expanded={expanded}
        onClick={() => setExpanded((current) => !current)}
      >
        <Text size="2" weight="medium">
          战斗状态覆盖{value.length > 0 ? ` (${value.length})` : ""}
        </Text>
      </button>
      {expanded && (
        <div className="battle-action-list">
          {relevant.map((entry) => {
            const key = `${entry.memberKey}:${entry.condition.stateKey}`;
            const active = entry.resolved?.activeConditions.find(
              (condition) => condition.stateKey === entry.condition.stateKey
            );
            return (
              <div className="battle-action-row committed" key={key}>
                <div className="battle-state-override-label">
                  <Text size="2" weight="medium">
                    {servantLabel(entry.index, entry.member.servant)} · {entry.condition.name}
                  </Text>
                  <Text size="1" color="gray">
                    {active
                      ? `当前有效${active.remainingTurns == null ? "" : `，剩余 ${active.remainingTurns} 回合`}${active.stacks > 1 ? `，${active.stacks} 层` : ""}`
                      : entry.condition.external
                        ? "外部条件，需手动设置"
                        : "可由已配置动作自动推演"}
                  </Text>
                </div>
                <label className="battle-state-number">
                  回合
                  <input
                    type="number"
                    min="1"
                    value={remainingTurns[key] ?? ""}
                    placeholder={entry.condition.durationTurns?.toString() ?? "持续"}
                    onChange={(event) =>
                      setRemainingTurns((current) => ({ ...current, [key]: event.target.value }))
                    }
                  />
                </label>
                <label className="battle-state-number">
                  层数
                  <input
                    type="number"
                    min="1"
                    max={entry.condition.maxStacks ?? undefined}
                    value={stacks[key] ?? ""}
                    placeholder="1"
                    onChange={(event) =>
                      setStacks((current) => ({ ...current, [key]: event.target.value }))
                    }
                  />
                </label>
                <Button type="button" size="1" onClick={() => updateOverride(entry, "set")}>
                  设置
                </Button>
                <Button
                  type="button"
                  size="1"
                  color="gray"
                  variant="soft"
                  onClick={() => updateOverride(entry, "clear")}
                >
                  清除
                </Button>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
