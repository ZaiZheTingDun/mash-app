import { BattleActorIcon } from "./BattleActorIcon";
import { convertFileSrc } from "../../tauri";
import type { MysticCode } from "../../types/mysticCode";

export function MysticCodeChoice({ code, selected = false, onClick }: {
  code?: MysticCode | null;
  selected?: boolean;
  onClick: () => void;
}) {
  const path = code?.itemFemalePath ?? code?.itemMalePath;
  return (
    <button type="button" className={`command-actor-choice command-mystic-choice${selected ? " selected" : ""}`} aria-label="御主礼装" aria-pressed={selected} onClick={onClick}>
      <BattleActorIcon kind="equipment" src={path ? convertFileSrc(path) : null} label="御主礼装" size="button" />
      <span className="command-actor-copy"><b>{code?.name ?? "御主礼装"}</b><small>御主礼装</small></span>
    </button>
  );
}
