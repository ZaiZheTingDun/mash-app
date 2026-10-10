import { useEffect, useState } from "react";
import { Avatar } from "@radix-ui/themes";
import { convertFileSrc, invoke } from "../../tauri";

// Indices follow get_attack_card_icon_paths: 宝具、红卡、蓝卡、绿卡、任意.
export function AttackCardOptionButtons<Value extends string>({ options, label, className = "", onSelect }: {
  options: { value: Value; label: string; iconIndex: number | null }[];
  label: string;
  className?: string;
  onSelect: (value: Value) => void;
}) {
  const [icons, setIcons] = useState<{ path: string | null; name: string }[]>([]);
  useEffect(() => {
    let cancelled = false;
    invoke<typeof icons>("get_attack_card_icon_paths")
      .then(entries => { if (!cancelled) setIcons(entries ?? []); })
      .catch(() => { if (!cancelled) setIcons([]); });
    return () => { cancelled = true; };
  }, []);

  return <div className={`battle-option-group command-skill-options ${className}`} role="group" aria-label={label}>
    {options.map(option => {
      const path = option.iconIndex == null ? null : icons[option.iconIndex]?.path;
      return <button type="button" key={option.value} className="battle-option-btn skill-icon command-skill-option"
        aria-label={option.label} title={option.label} onClick={() => onSelect(option.value)}>
        {option.iconIndex != null && <Avatar src={path ? convertFileSrc(path) : undefined} alt={option.label} fallback={option.label.slice(0, 1)} radius="small" size="3" />}
        <span className="command-skill-name">{option.label}</span>
      </button>;
    })}
  </div>;
}
