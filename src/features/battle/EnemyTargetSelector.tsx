import { SectionHeading } from "../../components/common/SectionHeading";
import { ENEMY_TARGETS, type EnemyTarget } from "./battleSceneModel";

interface EnemyTargetSelectorProps {
  value?: string | null;
  onChange: (value: EnemyTarget | null) => void;
  className?: string;
}

interface EnemyTargetButtonsProps {
  value?: string | null;
  onChange: (value: EnemyTarget | null) => void;
  ariaLabel?: string;
}

export function EnemyTargetButtons({
  value,
  onChange,
  ariaLabel = "敌方目标选择",
}: EnemyTargetButtonsProps) {
  return (
    <div className="battle-enemy-target-row" role="group" aria-label={ariaLabel}>
      <div className="battle-enemy-target-grid">
        {ENEMY_TARGETS.map((target) => (
          <button
            type="button"
            key={target.value}
            aria-label={target.label}
            aria-pressed={value === target.value}
            className={`battle-enemy-target${value === target.value ? " selected" : ""}`}
            onClick={() => onChange(value === target.value ? null : target.value)}
          >
            <span>{target.text.padStart(2, "0")}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

export function EnemyTargetSelector({
  value,
  onChange,
  className,
}: EnemyTargetSelectorProps) {
  return (
    <section className={`battle-phase command-target-selector${className ? ` ${className}` : ""}`}>
      <SectionHeading rail english="TARGET">敌方目标选择</SectionHeading>
      <div className="battle-action-list">
        <div className="battle-action-row committed">
          <span className="battle-action-delete-placeholder" aria-hidden />
          <EnemyTargetButtons value={value} onChange={onChange} />
          <button type="button" className="command-target-clear" disabled={!value} onClick={() => onChange(null)}>清除目标</button>
        </div>
      </div>
      <p className="command-target-hint">
        {value ? `当前目标：敌人 ${value.replace("enemy_", "")} · 再次点击已选位置可取消` : "当前未指定敌方目标"}
      </p>
    </section>
  );
}
