import { useCallback, useEffect, useRef, useState } from "react";
import { Box, Button, Dialog, Flex, Heading, IconButton, Select, Text } from "@radix-ui/themes";
import { Cross1Icon } from "@radix-ui/react-icons";
import { invoke } from "../../tauri";
import {
  DEFAULT_RECOGNITION_SETTINGS,
  normalizeRecognitionSettings,
  type ThresholdKey,
} from "../settings/recognitionSettingsModel";
import { RecognitionThresholdSettings } from "../settings/SettingsRecognitionPage";
import { ProjectRunSettings } from "./ProjectRunSettings";
import { DiamondSwitch } from "../../components/common/DiamondSwitch";
import { SettingsRow } from "../settings/SettingsRow";
import type { Project } from "../../types/project";
import type { ProjectRecognitionSettings, RecognitionSettings } from "../../types/recognition";

type ProjectSettingsSection = "basic" | "advanced" | "recognition";

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
  return Object.values(settings).some((value) => value != null);
}

export function ProjectSettingsDialog({
  open,
  project,
  onOpenChange,
  onUpdateProject,
}: ProjectSettingsDialogProps) {
  const [section, setSection] = useState<ProjectSettingsSection>("basic");
  const thresholdWriteProject = useRef(project);
  useEffect(() => { thresholdWriteProject.current = project; }, [project]);
  const [globalSettings, setGlobalSettings] = useState<RecognitionSettings>(
    DEFAULT_RECOGNITION_SETTINGS
  );
  const [settingsError, setSettingsError] = useState<string | null>(null);
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
      const currentProject = thresholdWriteProject.current;
      if (!currentProject) return DEFAULT_RECOGNITION_SETTINGS;
      const globalSettings = await invoke<RecognitionSettings>("get_recognition_settings");
      const currentOverrides = currentProject.recognitionSettings ?? {};
      const recognitionSettings = { ...currentOverrides };
      if (options?.restoreDefault) {
        delete recognitionSettings[key];
      } else {
        recognitionSettings[key] = value;
      }
      const nextRecognitionSettings = hasRecognitionOverrides(recognitionSettings)
        ? recognitionSettings
        : null;
      const nextProject = { ...currentProject, recognitionSettings: nextRecognitionSettings };
      await onUpdateProject(nextProject);
      thresholdWriteProject.current = nextProject;
      return projectRecognitionSettings(
        { ...currentProject, recognitionSettings: nextRecognitionSettings },
        globalSettings
      );
    },
    [onUpdateProject]
  );

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<RecognitionSettings>("get_recognition_settings")
      .then((settings) => {
        if (!cancelled) setGlobalSettings(normalizeRecognitionSettings(settings));
      })
      .catch((err) => {
        if (!cancelled) setSettingsError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const verifySkillActivationOverride =
    project?.recognitionSettings?.verifySkillActivation;
  const verifySkillActivationValue =
    verifySkillActivationOverride == null
      ? "inherit"
      : verifySkillActivationOverride
        ? "enabled"
        : "disabled";
  const inheritedVerifySkillActivationLabel = globalSettings.verifySkillActivation
    ? "开启"
    : "关闭";

  const saveVerifySkillActivationOverride = useCallback(
    async (value: string) => {
      if (!project) return;
      const recognitionSettings = { ...(project.recognitionSettings ?? {}) };
      if (value === "inherit") {
        delete recognitionSettings.verifySkillActivation;
      } else {
        recognitionSettings.verifySkillActivation = value === "enabled";
      }
      await onUpdateProject({
        ...project,
        recognitionSettings: hasRecognitionOverrides(recognitionSettings)
          ? recognitionSettings
          : null,
      });
    },
    [onUpdateProject, project]
  );

  const autoSkillTargetRecognitionValue = project?.disableAutoSkillTargetRecognition
    ? "disabled"
    : "enabled";
  const extraClassFilterOverride =
    project?.recognitionSettings?.enableExtraClassFilter;
  const extraClassFilterEnabled =
    extraClassFilterOverride ?? globalSettings.enableExtraClassFilter;

  const saveAutoSkillTargetRecognition = useCallback(
    async (value: string) => {
      if (!project) return;
      await onUpdateProject({
        ...project,
        disableAutoSkillTargetRecognition: value === "disabled",
      });
    },
    [onUpdateProject, project]
  );

  const saveExtraClassFilter = useCallback(
    async (enabled: boolean) => {
      if (!project) return;
      await onUpdateProject({
        ...project,
        recognitionSettings: {
          ...(project.recognitionSettings ?? {}),
          enableExtraClassFilter: enabled,
        },
      });
    },
    [onUpdateProject, project]
  );

  const savePreferHigherCriticalChance = useCallback(
    async (enabled: boolean) => {
      if (!project) return;
      await onUpdateProject({
        ...project,
        preferHigherCriticalChance: enabled,
      });
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
                    data-active={section === "basic" ? "true" : undefined}
                    aria-current={section === "basic" ? "page" : undefined}
                    onClick={() => setSection("basic")}
                    className="settings-nav-button"
                  >
                    <Text size="2" weight="medium">
                      基础设置
                    </Text>
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    color="gray"
                    data-active={section === "advanced" ? "true" : undefined}
                    aria-current={section === "advanced" ? "page" : undefined}
                    onClick={() => setSection("advanced")}
                    className="settings-nav-button"
                  >
                    <Text size="2" weight="medium">高级设置</Text>
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    color="gray"
                    data-active={section === "recognition" ? "true" : undefined}
                    aria-current={section === "recognition" ? "page" : undefined}
                    onClick={() => setSection("recognition")}
                    className="settings-nav-button"
                  >
                    <Text size="2" weight="medium">
                      阈值设置
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
                  {section === "basic" ? "基础设置" : section === "advanced" ? "高级设置" : "阈值设置"}
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
                {section === "basic" ? (
                  <Box className="settings-section-panel settings-basic">
                    <Flex direction="column" className="recognition-setting-block">
                      <section className="settings-basic-group" aria-label="运行">
                        <Heading as="h3" size="3" className="settings-basic-group-title">运行</Heading>
                        <SettingsRow label="额外职阶筛选"
                          description={<><strong>此为国服特有功能</strong>，助战为额外职阶时，可长按并选择具体职阶</>}
                          help={<Text size="1" color="gray">全局当前：{globalSettings.enableExtraClassFilter ? "开启" : "关闭"}</Text>}>
                          <DiamondSwitch checked={extraClassFilterEnabled}
                            onCheckedChange={(enabled) => void saveExtraClassFilter(enabled)}
                            aria-label="额外职阶筛选" />
                        </SettingsRow>
                        <SettingsRow label="优先选择暴击率更高的指令卡"
                          description="普通模式和高级模式中，同一成员有相同色卡时，优先选择暴击率更高的一张。">
                          <DiamondSwitch checked={project?.preferHigherCriticalChance ?? false}
                            onCheckedChange={(enabled) => void savePreferHigherCriticalChance(enabled)}
                            aria-label="优先选择暴击率更高的指令卡" />
                        </SettingsRow>
                        <ProjectRunSettings project={project} onUpdateProject={onUpdateProject} />
                      </section>
                    </Flex>
                  </Box>
                ) : section === "advanced" ? (
                  <Box className="settings-section-panel settings-basic">
                    <Flex direction="column" className="recognition-setting-block">
                      <SettingsRow label="技能使用确认"
                        description="开启后会确认技能使用成功，失败会进行重试"
                        help={<Text size="1" color="gray">全局当前：{inheritedVerifySkillActivationLabel}</Text>}>
                        <Select.Root value={verifySkillActivationValue}
                          onValueChange={(value) => void saveVerifySkillActivationOverride(value)}>
                          <Select.Trigger aria-label="队伍技能使用确认" className="recognition-mode-select" />
                          <Select.Content position="popper" className="settings-select-menu">
                            <Select.Item value="inherit">继承全局（{inheritedVerifySkillActivationLabel}）</Select.Item>
                            <Select.Item value="enabled">开启</Select.Item>
                            <Select.Item value="disabled">关闭</Select.Item>
                          </Select.Content>
                        </Select.Root>
                      </SettingsRow>
                      <SettingsRow label="关闭自动技能目标识别"
                        description="开启后选择技能时始终显示目标选择，不再根据技能数据自动跳过无目标技能。">
                        <Select.Root value={autoSkillTargetRecognitionValue}
                          onValueChange={(value) => void saveAutoSkillTargetRecognition(value)}>
                          <Select.Trigger aria-label="关闭自动技能目标识别" className="recognition-mode-select" />
                          <Select.Content position="popper" className="settings-select-menu">
                            <Select.Item value="enabled">关闭</Select.Item>
                            <Select.Item value="disabled">开启</Select.Item>
                          </Select.Content>
                        </Select.Root>
                      </SettingsRow>
                    </Flex>
                  </Box>
                ) : (
                  <RecognitionThresholdSettings
                    key={project?.id}
                    active={open && project != null}
                    helpText="此处只影响当前队伍；未保存队伍阈值时会继承全局设置。"
                    loadSettings={loadSettings}
                    loadDefaultSettings={loadDefaultSettings}
                    saveThreshold={saveThreshold}
                  />
                )}
                {settingsError && <Text as="div" size="1" color="red" mt="2">{settingsError}</Text>}
              </Box>
            </Box>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
