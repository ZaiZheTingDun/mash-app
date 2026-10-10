import { Avatar } from "@radix-ui/themes";

export function CommandSkillSummary({ src, name, fallback }: {
  src: string | null;
  name: string;
  fallback: string;
}) {
  return <>
    <span>释放</span>
    <span className="battle-inline-skill-icon" title={name}>
      <Avatar src={src ?? undefined} fallback={fallback} size="1" radius="small" />
    </span>
    <span className="command-row-skill-name" title={name}>{name}</span>
  </>;
}
