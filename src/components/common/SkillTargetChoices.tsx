import { Children, type CSSProperties, type ReactNode } from "react";
import orderChangeIcon from "../../../src-tauri/resources/images/icon_order_change.png";

interface SkillTargetChoicesProps {
  children: ReactNode;
  allowNoTarget: boolean;
  onNoTarget: () => void;
  onOrderChange?: () => void;
}

export function SkillTargetChoices({ children, allowNoTarget, onNoTarget, onOrderChange }: SkillTargetChoicesProps) {
  const optionCount = Children.count(children) + Number(allowNoTarget) + Number(Boolean(onOrderChange));
  return (
    <div className="command-target-options" role="group" aria-label="技能目标" style={{ "--target-option-count": optionCount } as CSSProperties}>
      {allowNoTarget && (
        <button type="button" className="command-actor-choice command-no-target-choice" aria-label="无目标" onClick={onNoTarget}>
          <span className="command-target-symbol" aria-hidden="true"><i className="command-diamond" /></span>
          <span className="command-actor-copy"><b>无目标</b><small>不指定技能目标</small></span>
        </button>
      )}
      {children}
      {onOrderChange && (
        <button type="button" className="command-actor-choice command-order-change-choice" aria-label="换人技能" onClick={onOrderChange}>
          <img className="command-target-skill-icon" src={orderChangeIcon} alt="" draggable={false} />
          <span className="command-actor-copy"><b>换人技能</b><small>选择前排与后排</small></span>
        </button>
      )}
    </div>
  );
}
