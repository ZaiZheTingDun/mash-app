import { BattleActorIcon } from "./BattleActorIcon";
import type { MysticCodeGender } from "../../types/appUiSettings";

export function CommandSpellChoice({ gender, selected = false, onClick }: {
  gender?: MysticCodeGender;
  selected?: boolean;
  onClick: () => void;
}) {
  return <button type="button" className={`command-actor-choice${selected ? " selected" : ""}`} aria-label="令咒" aria-pressed={selected} onClick={onClick}>
    <BattleActorIcon kind="commandSpell" label="令咒" gender={gender} size="button" />
    <span className="command-actor-copy"><b>令咒</b></span>
  </button>;
}
