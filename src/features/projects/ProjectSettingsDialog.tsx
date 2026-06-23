import { useCallback } from "react";
import { Box, Button, Dialog, Flex, IconButton, Text } from "@radix-ui/themes";
import { Cross1Icon, GearIcon } from "@radix-ui/react-icons";
import { invoke } from "../../tauri";
import {
  DEFAULT_RECOGNITION_SETTINGS,
  normalizeRecognitionSettings,
  type ThresholdKey,
} from "../settings/recognitionSettingsModel";
import { RecognitionThresholdSettings } from "../settings/SettingsRecognitionPage";
import type { Project } from "../../types/project";
import type { ProjectRecognitionSettings, RecognitionSettings } from "../../types/recognition";

interface ProjectSettingsDialogProps {
  open: boolean;
  project: Project | null;
  onOpenChange: (open: boolean) => void;
  onUpdateProject: (project: Project) => Promise<void>;
}

function projectRecognitionSettings(
  project: Project | null,
  globalSettings: RecognitionSettings
): RecognitionSettings {
  return normalizeRecognitionSettings(
    {
      ...(globalSettings ?? DEFAULT_RECOGNITION_SETTINGS),
      ...(project?.recognitionSettings ?? {}),
    }
  );
}

function hasRecognitionOverrides(settings: ProjectRecognitionSettings) {
  return Object.values(settings).some((value) => typeof value === "number");
}

export function ProjectSettingsDialog({
  open,
  project,
  onOpenChange,
  onUpdateProject,
}: ProjectSettingsDialogProps) {
  const loadSettings = useCallback(async () => {
    const globalSettings = await invoke<RecognitionSettings>("get_recognition_settings");
    return projectRecognitionSettings(project, globalSettings);
  }, [project]);
  const loadDefaultSettings = useCallback(
    () => invoke<RecognitionSettings>("get_recognition_settings"),
    []
  );

  const saveThreshold = useCallback(
    async (
      { key }: { key: ThresholdKey },
      value: number,
      options?: { restoreDefault?: boolean }
    ) => {
      if (!project) return DEFAULT_RECOGNITION_SETTINGS;
      const globalSettings = await invoke<RecognitionSettings>("get_recognition_settings");
      const currentOverrides = project.recognitionSettings ?? {};
      const recognitionSettings = { ...currentOverrides };
      if (options?.restoreDefault) {
        delete recognitionSettings[key];
      } else {
        recognitionSettings[key] = value;
      }
      const nextRecognitionSettings = hasRecognitionOverrides(recognitionSettings)
        ? recognitionSettings
        : null;
      await onUpdateProject({
        ...project,
        recognitionSettings: nextRecognitionSettings,
      });
      return projectRecognitionSettings(
        { ...project, recognitionSettings: nextRecognitionSettings },
        globalSettings
      );
    },
    [onUpdateProject, project]
  );

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content className="settings-dialog">
        <Flex className="settings-shell">
          <Flex asChild direction="column" className="settings-nav">
            <nav aria-label="队伍设置导航">
              <Box className="settings-nav-title">
                <Dialog.Title size="4">队伍设置</Dialog.Title>
              </Box>
              <Flex direction="column" gap="4">
                <Flex direction="column" gap="3" className="settings-nav-group">
                  <Text size="1" weight="bold" color="gray" className="settings-nav-group-label">
                    游戏
                  </Text>
                  <Button
                    type="button"
                    variant="ghost"
                    color="gray"
                    data-active="true"
                    aria-current="page"
                    className="settings-nav-button"
                  >
                    <GearIcon width={14} height={14} />
                    <Text size="2" weight="medium">
                      识别设置
                    </Text>
                  </Button>
                </Flex>
              </Flex>
              {project && (
                <Box className="team-settings-nav-project">
                  <Text size="1" color="gray">
                    {project.name}
                  </Text>
                </Box>
              )}
            </nav>
          </Flex>

          <Flex direction="column" className="settings-content">
            <Flex align="center" className="settings-content-header">
              <Flex direction="column" justify="center" className="settings-content-header-inner">
                <Text size="5" weight="bold">
                  识别设置
                </Text>
              </Flex>
              <Dialog.Close>
                <IconButton type="button" variant="ghost" color="gray" aria-label="关闭队伍设置">
                  <Cross1Icon width={15} height={15} />
                </IconButton>
              </Dialog.Close>
            </Flex>

            <Box className="settings-content-scroll">
              <Box className="settings-content-body">
                <RecognitionThresholdSettings
                  active={open && project != null}
                  helpText="此处只影响当前队伍；未保存队伍阈值时会继承全局设置。"
                  loadSettings={loadSettings}
                  loadDefaultSettings={loadDefaultSettings}
                  saveThreshold={saveThreshold}
                />
              </Box>
            </Box>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
