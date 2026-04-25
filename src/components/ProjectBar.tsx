import { useCallback } from "react";
import { Box, DropdownMenu, Flex, Text } from "@radix-ui/themes";
import {
  ChevronDownIcon,
  PlusIcon,
  TrashIcon,
  CheckIcon,
} from "@radix-ui/react-icons";
import type { Project } from "../types/project";

interface ProjectBarProps {
  projects: Project[];
  activeProjectId: string | null;
  onProjectSelect: (id: string) => void;
  onCreateProject: () => void;
  onDeleteProject: (id: string) => void;
}

/**
 * Mimics the FGO formation `~ 1 ~ PARTY` ribbon: a thin banner with two
 * decorative chevrons flanking a centered pill that opens a dropdown
 * listing every project alongside create/delete actions.
 *
 * Rendering keeps the bar visible across stages 1–3 so the user can
 * switch project without bouncing back to a sidebar. Project state
 * itself lives in `App.tsx`; this component is purely presentational.
 */
export function ProjectBar({
  projects,
  activeProjectId,
  onProjectSelect,
  onCreateProject,
  onDeleteProject,
}: ProjectBarProps) {
  const activeProject =
    projects.find((p) => p.id === activeProjectId) ?? null;

  const handleDeleteCurrent = useCallback(() => {
    if (!activeProject) return;
    // Lightweight destructive guard — matches the rest of the app, which
    // doesn't yet have a dedicated confirm-modal pattern.
    if (
      window.confirm(`确定删除项目「${activeProject.name}」吗？此操作无法撤销。`)
    ) {
      onDeleteProject(activeProject.id);
    }
  }, [activeProject, onDeleteProject]);

  const triggerLabel = activeProject?.name ?? "新建项目";

  return (
    <Box className="project-bar">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          <button type="button" className="project-bar-pill">
            <Text size="2" weight="medium">
              {`～ ${triggerLabel} ～`}
            </Text>
            <ChevronDownIcon width={14} height={14} />
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Content>
          {projects.length === 0 ? (
            <DropdownMenu.Item onSelect={onCreateProject}>
              <Flex align="center" gap="2">
                <PlusIcon width={12} height={12} />
                <Text size="2">新建项目</Text>
              </Flex>
            </DropdownMenu.Item>
          ) : (
            <>
              {projects.map((p) => {
                const isActive = p.id === activeProjectId;
                return (
                  <DropdownMenu.Item
                    key={p.id}
                    onSelect={() => onProjectSelect(p.id)}
                  >
                    <Flex align="center" gap="2" justify="between" width="100%">
                      <Text size="2">{p.name}</Text>
                      {isActive ? (
                        <CheckIcon width={14} height={14} />
                      ) : (
                        <span style={{ width: 14 }} />
                      )}
                    </Flex>
                  </DropdownMenu.Item>
                );
              })}
              <DropdownMenu.Separator />
              <DropdownMenu.Item onSelect={onCreateProject}>
                <Flex align="center" gap="2">
                  <PlusIcon width={12} height={12} />
                  <Text size="2">新建项目</Text>
                </Flex>
              </DropdownMenu.Item>
              <DropdownMenu.Item
                color="red"
                disabled={!activeProject}
                onSelect={handleDeleteCurrent}
              >
                <Flex align="center" gap="2">
                  <TrashIcon width={12} height={12} />
                  <Text size="2">删除当前项目</Text>
                </Flex>
              </DropdownMenu.Item>
            </>
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </Box>
  );
}
