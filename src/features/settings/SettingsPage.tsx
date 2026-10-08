import { useCallback, useEffect, useRef, useState } from "react";
import { Box, Button, Text } from "@radix-ui/themes";
import { PageHeader } from "../../components/common/PageHeader";
import { SectionHeading } from "../../components/common/SectionHeading";
import { invoke } from "../../tauri";
import { featureToggles } from "../../featureToggles";
import type { BattleStartPanel, MysticCodeGender } from "../../types/appUiSettings";
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

interface SettingsPageProps {
  open: boolean;
  section: SettingsSection;
  onOpenChange: (open: boolean) => void;
  onSectionChange: (section: SettingsSection) => void;
  onProjectsImported?: (projects: Project[]) => void;
  onBattleStartPanelChange?: (value: BattleStartPanel) => void;
  mysticCodeGender?: MysticCodeGender;
  onMysticCodeGenderChange?: (value: MysticCodeGender) => void;
}

type SettingsRenderProps = Pick<
  SettingsPageProps,
  "onProjectsImported" | "onBattleStartPanelChange" | "mysticCodeGender" | "onMysticCodeGenderChange"
> & {
  onResourcesExitBlockedChange: (blocked: boolean) => void;
};

const navItems: Array<{
  group: "game" | "application";
  section: SettingsSection;
  label: string;
  render: (active: boolean, props: SettingsRenderProps) => JSX.Element;
}> = [
  {
    group: "game",
    section: "basic",
    label: "基础设置",
    render: (active, props) => (
      <SettingsBasicPage
        active={active}
        onBattleStartPanelChange={props.onBattleStartPanelChange}
        mysticCodeGender={props.mysticCodeGender}
        onMysticCodeGenderChange={props.onMysticCodeGenderChange}
      />
    ),
  },
  {
    group: "game",
    section: "recognition",
    label: "阈值设置",
    render: (active) => <SettingsRecognitionPage active={active} />,
  },
  {
    group: "game",
    section: "dataManagement",
    label: "队伍管理",
    render: (_active, props) => (
      <SettingsDataManagementPage onProjectsImported={props.onProjectsImported} />
    ),
  },
  {
    group: "application",
    section: "debug",
    label: "调试",
    render: (active) => <SettingsDebugPage active={active} />,
  },
  {
    group: "application",
    section: "resources",
    label: "资源管理",
    render: (_active, props) => (
      <SettingsResourcesPage onExitBlockedChange={props.onResourcesExitBlockedChange} />
    ),
  },
  {
    group: "application",
    section: "selfCheck",
    label: "软件自检",
    render: (active) => <SettingsSelfCheckPage active={active} />,
  },
];

function visibleNavItems() {
  return navItems.filter((item) => item.section !== "debug" || featureToggles.settingsDebug);
}

export function SettingsPage({
  open,
  section,
  onOpenChange,
  onSectionChange,
  onProjectsImported,
  onBattleStartPanelChange,
  mysticCodeGender,
  onMysticCodeGenderChange,
}: SettingsPageProps) {
  const [resourcesExitBlocked, setResourcesExitBlocked] = useState(false);
  const pageRef = useRef<HTMLElement>(null);
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

  useEffect(() => {
    if (!open) return;
    const previousFocus = document.activeElement;
    pageRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => {
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      // Child menus/dialogs own their Escape key before it reaches this page.
      if (event.key === "Escape" && !event.defaultPrevented) handleOpenChange(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, handleOpenChange]);

  if (!open) return null;

  return (
    <main ref={pageRef} className="settings-page" aria-label="设置">
      <PageHeader title="设置" english="SETTINGS" onBack={() => handleOpenChange(false)}
        backLabel="关闭设置" backDisabled={activeSection === "resources" && resourcesExitBlocked}>
        {null}
      </PageHeader>
      <div className="settings-page-layout">
        <nav className="settings-page-nav" aria-label="设置导航">
          {groupedItems.map((group) => (
            <div key={group.group} className="settings-page-nav-group">
              <Text className="settings-page-nav-label">
                <span aria-hidden="true">{group.group === "game" ? "GAME / " : "APP / "}</span><span>{group.label}</span>
              </Text>
              {items.filter((item) => item.group === group.group).map((item) => (
                <Button key={item.section} type="button" variant="ghost" color="gray"
                  className="settings-page-nav-button"
                  data-active={activeSection === item.section ? "true" : undefined}
                  aria-current={activeSection === item.section ? "page" : undefined}
                  disabled={activeSection === "resources" && resourcesExitBlocked && item.section !== "resources"}
                  onClick={() => onSectionChange(item.section)}>
                  {item.label}
                </Button>
              ))}
            </div>
          ))}
        </nav>
        <div className="settings-page-scroll" key={activeSection} tabIndex={0}
          role="region" aria-label={`${activeItem.label}内容`}>
          <Box className="settings-page-body">
            <SectionHeading className="settings-page-section-title" english={
              { basic: "GENERAL", recognition: "RECOGNITION", dataManagement: "PARTY", debug: "DEBUG", resources: "RESOURCES", selfCheck: "SELF CHECK" }[activeSection]
            } stacked rail>{activeItem.label}</SectionHeading>
            {activeItem.render(open, {
              onProjectsImported, onBattleStartPanelChange, mysticCodeGender,
              onMysticCodeGenderChange, onResourcesExitBlockedChange: setResourcesExitBlocked,
            })}
          </Box>
        </div>
      </div>
    </main>
  );
}
