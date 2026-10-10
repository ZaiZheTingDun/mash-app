import { Avatar, Badge } from "@radix-ui/themes";
import { PersonIcon } from "@radix-ui/react-icons";
import type { BattleActorKind } from "./battleActorLabels";
import type { MysticCodeGender } from "../../types/appUiSettings";
import { CommandSpellIcon } from "./CommandSpellIcon";

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
  gender?: MysticCodeGender;
}

function iconClass(kind: BattleActorKind, size: BattleActorIconSize, className?: string): string {
  const base =
    size === "inline"
      ? kind === "servant"
        ? "battle-inline-face"
        : "battle-inline-square"
      : "battle-actor-icon";
  return [base, kind === "commandSpell" ? "battle-command-spell-icon" : "", className].filter(Boolean).join(" ");
}

export function BattleActorIcon({
  kind,
  src,
  label,
  isSupport = false,
  size = "inline",
  className,
  gender,
}: BattleActorIconProps) {
  const fallback =
    kind === "servant" ? (
      <PersonIcon width={size === "inline" ? 18 : 24} height={size === "inline" ? 18 : 24} aria-hidden />
    ) : (
      FALLBACK_BY_KIND[kind]
    );

  return (
    <span className={iconClass(kind, size, className)} aria-label={size === "inline" ? label : undefined}>
      {kind === "commandSpell" ? <CommandSpellIcon gender={gender} label={size === "inline" ? "" : label} /> : <Avatar
        src={src ?? undefined}
        alt={size === "inline" ? "" : label}
        radius="none"
        size={size === "inline" ? "2" : "4"}
        color={kind === "servant" ? undefined : "gray"}
        draggable={false}
        fallback={fallback}
      />}
      {kind === "servant" && isSupport && (
        <Badge className="battle-support-badge" color="gray" variant="surface" aria-label="助战">
          助
        </Badge>
      )}
    </span>
  );
}
