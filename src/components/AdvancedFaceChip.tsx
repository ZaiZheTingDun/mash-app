import { BattleActorIcon } from "./BattleActorIcon";
import { servantLabel } from "./battleActorLabels";
import type { Servant } from "../types/servant";

interface FaceChipProps {
  servant: Servant | null;
  index: number;
  src: string | null | undefined;
  active?: boolean;
  selected?: boolean;
  disabled?: boolean;
  isSupport?: boolean;
  onClick?: () => void;
}

export function FaceChip({
  servant,
  index,
  src,
  active = true,
  selected = false,
  disabled = false,
  isSupport = false,
  onClick,
}: FaceChipProps) {
  return (
    <button
      type="button"
      className={`advanced-face-chip${active ? "" : " dim"}${selected ? " selected" : ""}`}
      aria-label={servantLabel(index, servant)}
      disabled={disabled}
      onClick={onClick}
    >
      <BattleActorIcon
        kind="servant"
        src={src}
        label={servantLabel(index, servant)}
        isSupport={isSupport}
        size="button"
        className="advanced-face-chip-icon"
      />
    </button>
  );
}
