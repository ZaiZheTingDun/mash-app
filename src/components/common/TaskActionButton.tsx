import { Button } from "@radix-ui/themes";
import { ArrowRightIcon, StopIcon } from "@radix-ui/react-icons";

interface TaskActionButtonProps {
  label?: string;
  eyebrow?: string;
  running?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

export function TaskActionButton({
  running = false,
  disabled = false,
  label = running ? "停止任务" : "开始任务",
  eyebrow = running ? "STOP" : "START",
  onClick,
}: TaskActionButtonProps) {
  return (
    <Button
      type="button"
      aria-label={label}
      variant="ghost"
      color={running ? "red" : "blue"}
      className="task-action-button"
      disabled={disabled}
      onClick={onClick}
    >
      <span className="task-action-copy">
        <small>{eyebrow}</small>
        <strong>{label}</strong>
      </span>
      <span aria-hidden="true">{running ? <StopIcon /> : <ArrowRightIcon />}</span>
    </Button>
  );
}
