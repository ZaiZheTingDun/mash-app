import type { ReactNode } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";

interface SectionHeadingProps {
  children: ReactNode;
  className?: string;
  accessory?: ReactNode;
}

export function SectionHeading({ children, className, accessory }: SectionHeadingProps) {
  return (
    <Box className={`battle-section-heading${className ? ` ${className}` : ""}`}>
      <Flex align="center" gap="1">
        <Text size="4" weight="bold">
          {children}
        </Text>
        {accessory}
      </Flex>
    </Box>
  );
}
