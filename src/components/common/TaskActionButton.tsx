import { Button } from "@radix-ui/themes";

export function TaskActionButton({ running = false, disabled = false, onClick }: { running?: boolean; disabled?: boolean; onClick: () => void }) {
  return <Button type="button" aria-label={running ? "停止任务" : "开始任务"} variant="ghost" color={running ? "red" : "blue"} className="task-action-button" disabled={disabled} onClick={onClick}>
    <span className="task-action-copy"><small>{running ? "STOP" : "START"}</small><strong>{running ? "停止任务" : "开始任务"}</strong></span><span aria-hidden="true">{running ? "□" : "⟶"}</span>
  </Button>;
}
