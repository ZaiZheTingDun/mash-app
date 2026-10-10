import { ChevronRightIcon } from "@radix-ui/react-icons";

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
      aria-label={title}
      onClick={onClick}
    >
      <span className="home-action-orbit" aria-hidden="true" />
      <span className="home-action-copy">
        <span className="home-action-label" aria-hidden="true">
          <span className="home-action-number">{number}</span>
          <span className="home-action-diamond">◇</span>
          <span>{caption}</span>
        </span>
        <strong>{title}</strong>
        <small>{description}</small>
      </span>
      <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
    </button>
  );
}
