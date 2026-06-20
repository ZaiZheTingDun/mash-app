import { Avatar, Badge } from "@radix-ui/themes";
import { PersonIcon } from "@radix-ui/react-icons";
import type { BattleActorKind } from "./battleActorLabels";

const FALLBACK_BY_KIND: Record<BattleActorKind, string> = {
  servant: "",
  equipment: "御主",
  commandSpell: "令咒",
};

type BattleActorIconSize = "inline" | "button";

interface BattleActorIconProps {
  kind: BattleActorKind;
  src?: string | null;
  label: string;
  isSupport?: boolean;
  size?: BattleActorIconSize;
  className?: string;
}

function iconClass(kind: BattleActorKind, size: BattleActorIconSize, className?: string): string {
  const base =
    size === "inline"
      ? kind === "servant"
        ? "battle-inline-face"
        : "battle-inline-square"
      : "battle-actor-icon";
  return className ? `${base} ${className}` : base;
}

export function BattleActorIcon({
  kind,
  src,
  label,
  isSupport = false,
  size = "inline",
  className,
}: BattleActorIconProps) {
  const fallback =
    kind === "servant" ? (
      <PersonIcon width={size === "inline" ? 18 : 24} height={size === "inline" ? 18 : 24} aria-hidden />
    ) : (
      FALLBACK_BY_KIND[kind]
    );

  return (
    <span className={iconClass(kind, size, className)} aria-label={size === "inline" ? label : undefined}>
      <Avatar
        src={src ?? undefined}
        alt={size === "inline" ? "" : label}
        radius="none"
        size={size === "inline" ? "2" : "4"}
        color={kind === "servant" ? undefined : "gray"}
        draggable={false}
        fallback={fallback}
      />
      {kind === "servant" && isSupport && (
        <Badge className="battle-support-badge" color="gray" variant="surface" aria-label="助战">
          助
        </Badge>
      )}
    </span>
  );
}
