import type { ReactNode } from "react";
import { Box, Text } from "@radix-ui/themes";

interface SectionHeadingProps {
  children: ReactNode;
  className?: string;
}

export function SectionHeading({ children, className }: SectionHeadingProps) {
  return (
    <Box className={`battle-section-heading${className ? ` ${className}` : ""}`}>
      <Text size="4" weight="bold">
        {children}
      </Text>
    </Box>
  );
}
