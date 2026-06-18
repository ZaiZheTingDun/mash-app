import type { CSSProperties, ReactNode } from "react";
import { RadioGroup, Text } from "@radix-ui/themes";

export interface OptionCardRadioOption {
  value: string;
  title: ReactNode;
  ariaLabel?: string;
  description?: ReactNode;
  accessory?: ReactNode;
  className?: string;
  style?: CSSProperties;
  onSelect?: () => void;
}

interface OptionCardRadioGroupProps {
  value: string;
  options: OptionCardRadioOption[];
  disabled?: boolean;
  className?: string;
  onValueChange: (value: string) => void;
}

export function OptionCardRadioGroup({
  value,
  options,
  disabled = false,
  className,
  onValueChange,
}: OptionCardRadioGroupProps) {
  return (
    <RadioGroup.Root
      value={value}
      className={`option-card-radio-group${className ? ` ${className}` : ""}`}
      disabled={disabled}
      onValueChange={onValueChange}
    >
      {options.map((option) => {
        const selected = value === option.value;
        return (
          <div
            key={option.value}
            aria-disabled={disabled}
            aria-pressed={selected}
            className={`option-card-radio-item${selected ? " is-selected" : ""}${option.className ? ` ${option.className}` : ""}`}
            style={option.style}
            onClick={() => {
              if (disabled) return;
              option.onSelect?.();
              if (!option.onSelect) {
                onValueChange(option.value);
              }
            }}
          >
            <RadioGroup.Item
              value={option.value}
              className="option-card-radio-control"
              aria-label={option.ariaLabel ?? (typeof option.title === "string" ? option.title : undefined)}
            />
            <div className="option-card-radio-content">
              <Text size="3" weight="bold">
                {option.title}
              </Text>
              {option.description ? (
                <Text size="2" color="gray">
                  {option.description}
                </Text>
              ) : null}
            </div>
            {option.accessory}
          </div>
        );
      })}
    </RadioGroup.Root>
  );
}
