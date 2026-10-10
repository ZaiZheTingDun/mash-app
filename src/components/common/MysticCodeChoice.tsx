import { MysticCodeIcon } from "./MysticCodeIcon";
import type { MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";

export function MysticCodeChoice({ code, gender = "female", selected = false, onClick }: {
  code?: MysticCode | null;
  gender?: MysticCodeGender;
  selected?: boolean;
  onClick: () => void;
}) {
  return (
    <button type="button" className={`command-actor-choice command-mystic-choice${selected ? " selected" : ""}`} aria-label="御主礼装" title={code?.name ?? "御主礼装"} aria-pressed={selected} onClick={onClick}>
      <span className="battle-actor-icon">
        <MysticCodeIcon code={code} gender={gender} label="御主礼装" />
      </span>
      <span className="command-actor-copy"><b>{code?.name ?? "御主礼装"}</b><small>御主礼装</small></span>
    </button>
  );
}
