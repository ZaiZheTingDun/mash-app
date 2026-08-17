import { useCallback, useState } from "react";
import { Box, Button, Dialog, Flex, IconButton, Text } from "@radix-ui/themes";
import {
  ArchiveIcon,
  CheckCircledIcon,
  Cross1Icon,
  GearIcon,
  MixerHorizontalIcon,
} from "@radix-ui/react-icons";
import { invoke } from "../../tauri";
import { featureToggles } from "../../featureToggles";
import type { BattleStartPanel } from "../../types/appUiSettings";
import type { Project } from "../../types/project";
import { SettingsDataManagementPage } from "./SettingsDataManagementPage";
import { SettingsBasicPage } from "./SettingsBasicPage";
import { SettingsRecognitionPage } from "./SettingsRecognitionPage";
import { SettingsResourcesPage } from "./SettingsResourcesPage";
import { SettingsSelfCheckPage } from "./SettingsSelfCheckPage";
import { SettingsDebugPage } from "./SettingsDebugPage";

export type SettingsSection =
  | "basic"
  | "selfCheck"
  | "resources"
  | "dataManagement"
  | "recognition"
  | "debug";

interface SettingsDialogProps {
  open: boolean;
  section: SettingsSection;
  onOpenChange: (open: boolean) => void;
  onSectionChange: (section: SettingsSection) => void;
  onProjectsImported?: (projects: Project[]) => void;
  onBattleStartPanelChange?: (value: BattleStartPanel) => void;
}

type SettingsRenderProps = Pick<
  SettingsDialogProps,
  "onProjectsImported" | "onBattleStartPanelChange"
> & {
  onResourcesExitBlockedChange: (blocked: boolean) => void;
};

const navItems: Array<{
  group: "game" | "application";
  section: SettingsSection;
  label: string;
  icon: JSX.Element;
  render: (active: boolean, props: SettingsRenderProps) => JSX.Element;
}> = [
  {
    group: "game",
    section: "basic",
    label: "基础设置",
    icon: <GearIcon width={15} height={15} />,
    render: (active, props) => (
      <SettingsBasicPage
        active={active}
        onBattleStartPanelChange={props.onBattleStartPanelChange}
      />
    ),
  },
  {
    group: "game",
    section: "recognition",
    label: "阈值设置",
    icon: <MixerHorizontalIcon width={15} height={15} />,
    render: (active) => <SettingsRecognitionPage active={active} />,
  },
  {
    group: "game",
    section: "dataManagement",
    label: "队伍管理",
    icon: <ArchiveIcon width={15} height={15} />,
    render: (_active, props) => (
      <SettingsDataManagementPage onProjectsImported={props.onProjectsImported} />
    ),
  },
  {
    group: "application",
    section: "debug",
    label: "调试",
    icon: <GearIcon width={15} height={15} />,
    render: (active) => <SettingsDebugPage active={active} />,
  },
  {
    group: "application",
    section: "resources",
    label: "资源管理",
    icon: <ArchiveIcon width={15} height={15} />,
    render: (_active, props) => (
      <SettingsResourcesPage onExitBlockedChange={props.onResourcesExitBlockedChange} />
    ),
  },
  {
    group: "application",
    section: "selfCheck",
    label: "软件自检",
    icon: <CheckCircledIcon width={15} height={15} />,
    render: (active) => <SettingsSelfCheckPage active={active} />,
  },
];

function visibleNavItems() {
  return navItems.filter((item) => item.section !== "debug" || featureToggles.settingsDebug);
}

export function SettingsDialog({
  open,
  section,
  onOpenChange,
  onSectionChange,
  onProjectsImported,
  onBattleStartPanelChange,
}: SettingsDialogProps) {
  const [resourcesExitBlocked, setResourcesExitBlocked] = useState(false);
  const items = visibleNavItems();
  const activeItem = items.find((item) => item.section === section) ?? items[0];
  const activeSection = activeItem.section;
  const groupedItems = [
    { group: "game", label: "游戏" },
    { group: "application", label: "应用" },
  ] as const;

  const handleOpenChange = useCallback((nextOpen: boolean) => {
    if (!nextOpen && section === "resources" && resourcesExitBlocked) {
      return;
    }
    if (!nextOpen && section === "resources") {
      void invoke("cancel_resource_downloads").catch((err) => {
        console.error("cancel_resource_downloads failed", err);
      });
    }
    onOpenChange(nextOpen);
  }, [onOpenChange, resourcesExitBlocked, section]);

  return (
    <Dialog.Root open={open} onOpenChange={handleOpenChange}>
      <Dialog.Content className="settings-dialog">
        <Flex className="settings-shell">
          <Flex asChild direction="column" className="settings-nav">
            <nav aria-label="设置导航">
              <Box className="settings-nav-title">
                <Dialog.Title size="4">设置</Dialog.Title>
              </Box>
              <Flex direction="column" gap="4">
                {groupedItems.map((group) => (
                  <Flex key={group.group} direction="column" gap="3" className="settings-nav-group">
                    <Text size="1" weight="bold" color="gray" className="settings-nav-group-label">
                      {group.label}
                    </Text>
                    {items
                      .filter((item) => item.group === group.group)
                      .map((item) => (
                        <Button
                          key={item.section}
                          type="button"
                          variant="ghost"
                          color="gray"
                          data-active={activeSection === item.section ? "true" : undefined}
                          aria-current={activeSection === item.section ? "page" : undefined}
                          disabled={
                            activeSection === "resources" &&
                            resourcesExitBlocked &&
                            item.section !== "resources"
                          }
                          onClick={() => onSectionChange(item.section)}
                          className="settings-nav-button"
                        >
                          {item.icon}
                          <Text size="2" weight="medium">
                            {item.label}
                          </Text>
                        </Button>
                      ))}
                  </Flex>
                ))}
              </Flex>
            </nav>
          </Flex>

          <Flex direction="column" className="settings-content">
            <Flex align="center" className="settings-content-header">
              <Flex align="center" className="settings-content-header-inner">
                <Text size="5" weight="bold">
                  {activeItem.label}
                </Text>
              </Flex>
              <Dialog.Close>
                <IconButton
                  type="button"
                  variant="ghost"
                  color="gray"
                  aria-label="关闭设置"
                  disabled={activeSection === "resources" && resourcesExitBlocked}
                >
                  <Cross1Icon width={15} height={15} />
                </IconButton>
              </Dialog.Close>
            </Flex>

            <Box className="settings-content-scroll">
              <Box className="settings-content-body">
                {activeItem.render(open && activeItem.section === activeSection, {
                  onProjectsImported,
                  onBattleStartPanelChange,
                  onResourcesExitBlockedChange: setResourcesExitBlocked,
                })}
              </Box>
            </Box>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
