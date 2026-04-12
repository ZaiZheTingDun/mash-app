import { Box, Text, Flex } from "@radix-ui/themes";

interface StageNavigatorProps {
  activeStage: number;
  onStageChange: (stage: number) => void;
}

const stages = [
  { id: 1, label: "队伍设置" },
  { id: 2, label: "助战设置" },
  { id: 3, label: "指令设置" },
];

export function StageNavigator({
  activeStage,
  onStageChange,
}: StageNavigatorProps) {
  return (
    <Flex align="center" justify="center" gap="2" className="stage-navigator">
      {stages.map((stage, index) => (
        <Flex key={stage.id} align="center" gap="2">
          <button
            className={`stage-button ${activeStage === stage.id ? "active" : ""}`}
            onClick={() => onStageChange(stage.id)}
          >
            <Flex align="center" gap="2" className="stage-button-inner">
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
