import { Box, Text, Flex } from "@radix-ui/themes";

interface StageNavigatorProps {
  activeStage: number;
  onStageChange: (stage: number) => void;
}

const stages = [
  { id: 1, label: "Stage 1" },
  { id: 2, label: "Stage 2" },
  { id: 3, label: "Stage 3" },
  { id: 4, label: "Stage 4" },
];

export function StageNavigator({
  activeStage,
  onStageChange,
}: StageNavigatorProps) {
  return (
    <Flex align="center" gap="2" className="stage-navigator">
      {stages.map((stage, index) => (
        <Flex key={stage.id} align="center" gap="2">
          <button
            className={`stage-button ${activeStage === stage.id ? "active" : ""}`}
            onClick={() => onStageChange(stage.id)}
          >
            <Flex align="center" gap="2" className="stage-button-inner">
              <Box
                className={`stage-number ${activeStage === stage.id ? "active" : ""}`}
              >
                <Text size="1" weight="bold">
                  {stage.id}
                </Text>
              </Box>
              <Text size="2" weight="medium">
                {stage.label}
              </Text>
            </Flex>
          </button>
          {index < stages.length - 1 && <Box className="stage-divider" />}
        </Flex>
      ))}
    </Flex>
  );
}
