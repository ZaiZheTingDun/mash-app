import { Button } from "@radix-ui/themes";

interface StopAfterRoundButtonProps {
  selected?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

export function StopAfterRoundButton({
  selected = false,
  disabled = false,
  onClick,
}: StopAfterRoundButtonProps) {
  const label = selected ? "本轮结束后将停止" : "运行完当前轮次后停止";

  return (
    <Button
      type="button"
      variant="ghost"
      color={selected ? "red" : "gray"}
      className="stop-after-round-button"
      aria-label={label}
      aria-pressed={selected}
      data-selected={selected}
      disabled={disabled || selected}
      onClick={onClick}
    >
      <svg className="stop-after-round-diamond" viewBox="0 0 26 26" aria-hidden="true">
        <path d="M13 1.5 24.5 13 13 24.5 1.5 13Z" />
      </svg>
      <span className="stop-after-round-copy">
        <small>{selected ? "STOPPING AFTER ROUND" : "STOP AFTER ROUND"}</small>
        <span>{label}</span>
      </span>
    </Button>
  );
}
