import type { ComponentProps } from "react";
import { Switch } from "@radix-ui/themes";

/** 菱形轴线开关；沿用 Radix 的键盘、焦点和禁用语义。 */
export function DiamondSwitch({ className, ...props }: ComponentProps<typeof Switch>) {
  return <Switch {...props} className={`diamond-switch${className ? ` ${className}` : ""}`} />;
}
