import * as RadioGroup from "@radix-ui/react-radio-group";
import type React from "react";

/**
 * Generic single-choice "minimum threshold" picker — a horizontal grid
 * of `[任意, 1, 2, …, maxLevel]` tiles where each tile is colored by
 * its relationship to the current selection:
 *
 *   - `current` — the tile that matches `value`
 *   - `meets`   — strictly greater than `value` (would also satisfy
 *                 the threshold)
 *   - `hint`    — strictly less than `value` (would not satisfy)
 *
 * Built on `@radix-ui/react-radio-group` so the group exposes
 * `role="radiogroup"`, items expose `role="radio"` + `aria-checked`,
 * and arrow-key navigation / roving tabindex come for free. Visual
 * state is layered on top via className.
 *
 * Pure presentational component — drives nothing on its own; the
 * caller owns the `value` state and decides whether to commit it
 * (e.g. behind a confirm/cancel dialog).
 */

interface ThresholdLevelPickerProps {
  /** Currently selected threshold; `null` means "任意" (no minimum). */
  value: number | null;
  /** Inclusive upper bound; tiles render `1..maxLevel`. */
  maxLevel: number;
  /** Accessible name for the radiogroup container. */
  ariaLabel: string;
  onChange: (value: number | null) => void;
}

/**
 * Map a tile's level to its visual-state className. Kept module-private:
 * the picker is the sole consumer and the exact taxonomy is an
 * implementation detail of this component pair.
 */
function thresholdLevelPickerItemClass(
  level: number | "any",
  currentLevel: number | null,
) {
  const classes = ["support-level-picker-item"];
  if (level === "any") {
    classes.push(currentLevel == null ? "current" : "hint");
  } else if (currentLevel == null || level < currentLevel) {
    classes.push("hint");
  } else if (level === currentLevel) {
    classes.push("current");
  } else {
    classes.push("meets");
  }
  return classes.join(" ");
}

function thresholdMatchSummary(value: number | null, maxLevel: number) {
  if (value == null) return "匹配任意等级";
  if (value === maxLevel) return `将匹配 ${maxLevel}`;
  return `将匹配 ${value}～${maxLevel}`;
}

export function ThresholdLevelPicker({
  value,
  maxLevel,
  ariaLabel,
  onChange,
}: ThresholdLevelPickerProps) {
  const levels = Array.from({ length: maxLevel }, (_, index) => index + 1);
  const gridStyle = {
    "--threshold-level-count": maxLevel,
  } as React.CSSProperties;
  const radioValue = value == null ? "any" : String(value);

  return (
    <RadioGroup.Root
      className="support-level-picker"
      aria-label={ariaLabel}
      style={gridStyle}
      value={radioValue}
      onValueChange={(next) => onChange(next === "any" ? null : Number(next))}
    >
      <RadioGroup.Item
        value="any"
        className={thresholdLevelPickerItemClass("any", value)}
      >
        任意
      </RadioGroup.Item>
      {levels.map((level) => (
        <RadioGroup.Item
          key={level}
          value={String(level)}
          className={thresholdLevelPickerItemClass(level, value)}
        >
          {level}
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}

interface ThresholdLevelLegendProps {
  value: number | null;
  maxLevel: number;
}

/**
 * Three-cell legend that pairs with `ThresholdLevelPicker`:
 *
 *   [hint key]  [current key]  [live "将匹配 …" summary]
 *
 * The two static keys are decorative (screen readers already learn the
 * tile state from `aria-checked`), so they're hidden from AT. The
 * right-hand cell is informational and announces politely as the
 * threshold changes.
 */
export function ThresholdLevelLegend({
  value,
  maxLevel,
}: ThresholdLevelLegendProps) {
  return (
    <div className="support-level-range-labels">
      <span className="hint" aria-hidden>将匹配所选等级及以上</span>
      <span className="current" aria-hidden>当前选择</span>
      <span className="meets" aria-live="polite">
        {thresholdMatchSummary(value, maxLevel)}
      </span>
    </div>
  );
}
