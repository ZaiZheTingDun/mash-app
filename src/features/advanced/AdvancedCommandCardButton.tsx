import type { CSSProperties } from "react";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { servantLabel } from "../../components/common/battleActorLabels";
import {
  COMMAND_BG_BY_SUIT,
  commandCardAria,
  servantSlotIndex,
} from "./advancedCommandModel";
import type { PartyMember } from "../team/partyServants";
import type { AdvancedCommandCardCondition } from "../../types/command";
import commandBgArts from "../../../src-tauri/resources/images/command_bg/command_bg_a.png";

interface AdvancedCommandCardButtonProps {
  card: AdvancedCommandCardCondition;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onClick: () => void;
}

export function AdvancedCommandCardButton({
  card,
  partyMembers,
  faces,
  onClick,
}: AdvancedCommandCardButtonProps) {
  const servantIndex = servantSlotIndex(card.servant);
  const member = servantIndex == null ? null : partyMembers[servantIndex] ?? null;
  const servant = member?.servant ?? null;
  const faceSrc = servant ? faces[servant.variantKey] : null;
  const unset = card.servant === "any" && card.suit === "any";
  const grayscale = card.suit === "any";
  const style = {
    "--command-card-bg": `url(${card.suit === "any" ? commandBgArts : COMMAND_BG_BY_SUIT[card.suit]})`,
  } as CSSProperties;

  return (
    <button
      type="button"
      className={`advanced-command-card${grayscale ? " unset" : ""}`}
      style={style}
      aria-label={commandCardAria(card)}
      onClick={onClick}
    >
      {!unset && servantIndex != null && (
        <BattleActorIcon
          kind="servant"
          src={faceSrc}
          label={servantLabel(servantIndex, servant)}
          isSupport={member?.isSupport ?? false}
          size="button"
          className="advanced-command-card-face"
        />
      )}
    </button>
  );
}
