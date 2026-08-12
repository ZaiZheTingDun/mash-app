import type { ButtonHTMLAttributes, ReactNode } from "react";
import { PlusIcon } from "@radix-ui/react-icons";
import { Text } from "@radix-ui/themes";

interface AddRowTriggerProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  children: ReactNode;
  leading?: ReactNode;
  transparentIconBackground?: boolean;
  iconSize?: number;
}

export function AddRowTrigger({
  children,
  leading,
  transparentIconBackground = false,
  iconSize = 48,
  className,
  type = "button",
  ...buttonProps
}: AddRowTriggerProps) {
  return (
    <button
      {...buttonProps}
      type={type}
      className={`add-row-trigger${transparentIconBackground ? " is-icon-background-transparent" : ""}${className ? ` ${className}` : ""}`}
    >
      {leading}
      <span
        className="add-row-trigger-icon"
        aria-hidden
        style={{ width: iconSize, height: iconSize, flexBasis: iconSize }}
      >
        <PlusIcon width={16} height={16} />
      </span>
      <Text as="span" size="2" weight="medium">
        {children}
      </Text>
    </button>
  );
}
