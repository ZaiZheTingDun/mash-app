import { Avatar } from "@radix-ui/themes";
import { convertFileSrc } from "../../tauri";
import { mysticCodeItemPath, type MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";

export function MysticCodeIcon({ code, gender = "female", label, size = "3", inline = false }: {
  code?: MysticCode | null;
  gender?: MysticCodeGender;
  label: string;
  size?: "2" | "3" | "5";
  inline?: boolean;
}) {
  const path = code ? mysticCodeItemPath(code, gender) : null;
  const avatar = <Avatar
    className="mystic-code-icon"
    src={path ? convertFileSrc(path) : undefined}
    fallback="礼"
    alt={inline ? "" : label}
    radius="full"
    size={size}
    color="blue"
    draggable={false}
  />;
  return inline ? <span className="battle-inline-square battle-mystic-code-icon" aria-label={label}>{avatar}</span> : avatar;
}
