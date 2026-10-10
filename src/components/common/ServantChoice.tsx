import { BattleActorIcon } from "./BattleActorIcon";
import { servantLabel } from "./battleActorLabels";
import type { Servant } from "../../types/servant";

export function ServantChoice({ servant, index, src, isSupport = false, selected = false, disabled = false, className = "", onClick }: { servant: Servant | null; index: number; src?: string | null; isSupport?: boolean; selected?: boolean; disabled?: boolean; className?: string; onClick?: () => void }) {
  const label = servant ? `${servantLabel(index, servant)} · 位置 ${index + 1}${isSupport ? " · 助战" : ""}` : `位置 ${index + 1} 未配置从者`;
  return <button type="button" className={`command-actor-choice${selected ? " selected" : ""}${!servant ? " is-empty" : ""} ${className}`} title={label} aria-label={servant ? servantLabel(index, servant) : label} aria-pressed={selected} disabled={disabled || !servant} onClick={onClick}>
    {servant ? <BattleActorIcon kind="servant" src={src} label={label} isSupport={isSupport} size="button" /> : <span className="command-empty-face" aria-hidden="true"><i className="command-diamond" /></span>}
    <span className="command-actor-copy"><b>{servant?.name_cn ?? "未配置从者"}</b><small>{index < 3 ? "前排" : "后排"} · 位置 {index + 1}</small></span>
  </button>;
}
