import { ServantChoice } from "../../components/common/ServantChoice";
import type { Servant } from "../../types/servant";

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
  return <ServantChoice servant={servant} index={index} src={src} isSupport={isSupport} selected={selected} disabled={disabled || !active} onClick={onClick} className={`advanced-face-chip${active ? "" : " dim"}`} />;
}
