import { ChevronRightIcon, TargetIcon, StarIcon, ThickArrowUpIcon } from "@radix-ui/react-icons";

interface TaskCardProps {
  appearance: "battle" | "summon" | "enhance";
  number: string;
  caption: string;
  title: string;
  description: string;
  onClick: () => void;
}

/** 主页任务入口；主题由应用现有的浅色 / 深色设置控制。 */
export function TaskCard({ appearance, number, caption, title, description, onClick }: TaskCardProps) {
  return (
    <button
      type="button"
      className={`home-action home-action--${appearance}`}
      data-task-number={number}
      data-task-caption={caption}
      aria-label={title}
      onClick={onClick}
    >
      <span className="home-action-icon" aria-hidden="true">
        {appearance === "battle" ? <TargetIcon /> : appearance === "summon" ? <StarIcon /> : <ThickArrowUpIcon />}
      </span>
      <span className="home-action-copy">
        <strong>{title}</strong>
        <small>{description}</small>
      </span>
      <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
    </button>
  );
}
