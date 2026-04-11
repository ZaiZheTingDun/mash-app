import { Box, Flex } from "@radix-ui/themes";
import { ImageIcon } from "@radix-ui/react-icons";

function ImageCard() {
  return (
    <Box className="image-card">
      <Flex align="center" justify="center" className="image-card-inner">
        <ImageIcon width={36} height={36} className="image-card-icon" />
      </Flex>
    </Box>
  );
}

export function ContentGrid() {
  return (
    <Flex gap="6" className="content-grid-wrapper">
      <Box className="content-grid">
        {Array.from({ length: 6 }).map((_, i) => (
          <ImageCard key={`left-${i}`} />
        ))}
      </Box>

      <Box className="content-divider" />

      <Box className="content-grid">
        {Array.from({ length: 6 }).map((_, i) => (
          <ImageCard key={`right-${i}`} />
        ))}
      </Box>
    </Flex>
  );
}
