import type { ReactNode } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { SectionRailIcon } from "./SectionRailIcon";

interface SectionHeadingProps {
  children: ReactNode;
  className?: string;
  accessory?: ReactNode;
  rail?: boolean;
  english?: string;
  stacked?: boolean;
}

export function SectionHeading({ children, className, accessory, rail = false, english, stacked = false }: SectionHeadingProps) {
  return (
    <Box className={`battle-section-heading${className ? ` ${className}` : ""}${stacked ? " section-heading-stacked" : ""}`}>
      <Flex align="center" gap="1">
        {rail && <SectionRailIcon stem={stacked} />}
        {stacked ? <span className="section-heading-labels">
          {english && <span className="section-heading-english">{english}</span>}
          <Text size="4" weight="bold">{children}</Text>
        </span> : <>
          <Text size="4" weight="bold">{children}</Text>
          {english && <span className="section-heading-english">{english}</span>}
        </>}
        {accessory}
      </Flex>
    </Box>
  );
}
