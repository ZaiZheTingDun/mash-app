import { useState } from "react";
import { Box, Button, Flex, Select, Switch, Text, TextField } from "@radix-ui/themes";
import { MinusIcon, PlusIcon } from "@radix-ui/react-icons";
import type { Project } from "../../types/project";
import type { ProjectRecognitionSettings, SupportClassFilterMode } from "../../types/recognition";

interface ProjectRunSettingsProps {
  project: Project | null;
  onUpdateProject: (project: Project) => Promise<void>;
}

export function ProjectRunSettings({ project, onUpdateProject }: ProjectRunSettingsProps) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const settings = project?.recognitionSettings;
  const targetCount = settings?.fiveStarCeDropTargetCount ?? 1;
  const disabled = !project || saving;

  const saveSettings = async (patch: Partial<ProjectRecognitionSettings>) => {
    if (!project || saving) return;
    setSaving(true);
    setError(null);
    try {
      await onUpdateProject({
        ...project,
        recognitionSettings: { ...project.recognitionSettings, ...patch },
      });
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Box className="settings-section-panel">
      <Flex direction="column" gap="4" className="recognition-setting-block">
        <Flex align="center" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Text size="2" weight="bold">助战筛选设置</Text>
          <Select.Root
            value={settings?.supportClassFilterMode ?? "correspondingClass"}
            disabled={disabled}
            onValueChange={(value) => void saveSettings({
              supportClassFilterMode: value as SupportClassFilterMode,
            })}
          >
            <Select.Trigger aria-label="助战筛选设置" />
            <Select.Content>
              <Select.Item value="correspondingClass">只筛选对应职阶</Select.Item>
              <Select.Item value="allClasses">只筛选全职阶</Select.Item>
              <Select.Item value="both">同时筛选</Select.Item>
            </Select.Content>
          </Select.Root>
        </Flex>

        <Flex align="start" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">五星礼装掉落自动停止</Text>
            <Text size="1" color="gray">
              开启后在战利品结算页检测前两行，累计达到目标数量后停止
            </Text>
          </Flex>
          <Switch
            checked={settings?.stopOnFiveStarCeDrop === true}
            onCheckedChange={(value) => void saveSettings({ stopOnFiveStarCeDrop: value })}
            disabled={disabled}
            aria-label="五星礼装掉落自动停止"
          />
        </Flex>

        {settings?.stopOnFiveStarCeDrop && (
          <Flex align="center" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
            <Flex direction="column" gap="1" className="basic-setting-copy">
              <Text size="2" weight="bold">掉落个数</Text>
              <Text size="1" color="gray">本次自动化运行内累计计算，默认 1</Text>
            </Flex>
            <Flex align="center" gap="2" className="battle-repeat-counter">
              <Button
                type="button"
                color="indigo"
                disabled={disabled || targetCount <= 1}
                aria-label="减少五星礼装掉落个数"
                onClick={() => void saveSettings({ fiveStarCeDropTargetCount: targetCount - 1 })}
              >
                <MinusIcon width={15} height={15} />
              </Button>
              <TextField.Root
                className="battle-counter-value"
                type="number"
                min="1"
                max="4294967295"
                step="1"
                variant="soft"
                radius="none"
                inputMode="numeric"
                aria-label="五星礼装掉落个数"
                value={targetCount}
                disabled={disabled}
                onChange={(event) => {
                  if (event.currentTarget.value !== "" && event.currentTarget.validity.valid) {
                    void saveSettings({ fiveStarCeDropTargetCount: event.currentTarget.valueAsNumber });
                  }
                }}
              />
              <Button
                type="button"
                color="indigo"
                disabled={disabled || targetCount >= 4_294_967_295}
                aria-label="增加五星礼装掉落个数"
                onClick={() => void saveSettings({ fiveStarCeDropTargetCount: targetCount + 1 })}
              >
                <PlusIcon width={15} height={15} />
              </Button>
            </Flex>
          </Flex>
        )}
        {error && <Text size="1" color="red" role="alert">{error}</Text>}
      </Flex>
    </Box>
  );
}
