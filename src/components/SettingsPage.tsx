import { useCallback } from "react";
import { Box, Button, Dialog, Flex, IconButton, Text } from "@radix-ui/themes";
import { ArchiveIcon, CheckCircledIcon, Cross1Icon } from "@radix-ui/react-icons";
import { invoke } from "../tauri";
import type { Project } from "../types/project";
import { SettingsDataManagementPage } from "./SettingsDataManagementPage";
import { SettingsResourcesPage } from "./SettingsResourcesPage";
import { SettingsSelfCheckPage } from "./SettingsSelfCheckPage";

export type SettingsSection = "selfCheck" | "resources" | "dataManagement";

interface SettingsDialogProps {
  open: boolean;
  section: SettingsSection;
  onOpenChange: (open: boolean) => void;
  onSectionChange: (section: SettingsSection) => void;
  onProjectsImported?: (projects: Project[]) => void;
}

const navItems: Array<{
  group: "game" | "application";
  section: SettingsSection;
  label: string;
  icon: JSX.Element;
  render: (active: boolean, props: Pick<SettingsDialogProps, "onProjectsImported">) => JSX.Element;
}> = [
    {
      group: "game",
      section: "dataManagement",
      label: "数据管理",
      icon: <ArchiveIcon width={15} height={15} />,
      render: (_active, props) => (
        <SettingsDataManagementPage onProjectsImported={props.onProjectsImported} />
      ),
    },
    {
      group: "application",
      section: "resources",
      label: "资源管理",
      icon: <ArchiveIcon width={15} height={15} />,
      render: () => <SettingsResourcesPage />,
    },
    {
      group: "application",
      section: "selfCheck",
      label: "软件自检",
      icon: <CheckCircledIcon width={15} height={15} />,
      render: (active) => <SettingsSelfCheckPage active={active} />,
    },
  ];

export function SettingsDialog({
  open,
  section,
  onOpenChange,
  onSectionChange,
  onProjectsImported,
}: SettingsDialogProps) {
  const activeItem = navItems.find((item) => item.section === section) ?? navItems[0];
  const groupedItems = [
    { group: "game", label: "游戏" },
    { group: "application", label: "应用" },
  ] as const;

  const handleOpenChange = useCallback((nextOpen: boolean) => {
    if (!nextOpen && section === "resources") {
      void invoke("cancel_resource_downloads").catch((err) => {
        console.error("cancel_resource_downloads failed", err);
      });
    }
    onOpenChange(nextOpen);
  }, [onOpenChange, section]);

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
                    {navItems
                      .filter((item) => item.group === group.group)
                      .map((item) => (
                        <Button
                          key={item.section}
                          type="button"
                          variant="ghost"
                          color="gray"
                          data-active={section === item.section ? "true" : undefined}
                          aria-current={section === item.section ? "page" : undefined}
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
            <Flex align="center" justify="between" className="settings-content-header">
              <Text size="5" weight="bold">
                {activeItem.label}
              </Text>
              <Dialog.Close>
                <IconButton type="button" variant="ghost" color="gray" aria-label="关闭设置">
                  <Cross1Icon width={15} height={15} />
                </IconButton>
              </Dialog.Close>
            </Flex>

            <Box className="settings-content-scroll">
              {activeItem.render(open && activeItem.section === section, { onProjectsImported })}
            </Box>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
