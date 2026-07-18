import { ENEMY_TARGETS, type EnemyTarget } from "./battleSceneModel";

interface EnemyTargetSelectorProps {
  value?: string | null;
  onChange: (value: EnemyTarget | null) => void;
  className?: string;
}

export function EnemyTargetSelector({
  value,
  onChange,
  className,
}: EnemyTargetSelectorProps) {
  return (
    <section className={`battle-phase${className ? ` ${className}` : ""}`}>
      <div className="battle-phase-label">敌方目标选择</div>
      <div className="battle-action-list">
        <div className="battle-action-row committed">
          <span className="battle-action-delete-placeholder" aria-hidden />
          <div
            className="battle-enemy-target-row"
            role="group"
            aria-label="敌方目标选择"
          >
            <div className="battle-enemy-target-grid">
              {ENEMY_TARGETS.map((target) => (
                <button
                  type="button"
                  key={target.value}
                  aria-label={target.label}
                  className={`battle-enemy-target${value === target.value ? " selected" : ""}`}
                  onClick={() =>
                    onChange(value === target.value ? null : target.value)
                  }
                >
                  <span>{target.text}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
