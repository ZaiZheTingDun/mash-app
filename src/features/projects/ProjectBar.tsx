import { useCallback, useMemo, useState } from "react";
import type { FormEvent } from "react";
import {
  AlertDialog,
  Box,
  Button,
  Dialog,
  DropdownMenu,
  Flex,
  IconButton,
  Select,
  Text,
  TextField,
} from "@radix-ui/themes";
import {
  CopyIcon,
  DotsHorizontalIcon,
  Pencil1Icon,
  PlusIcon,
  GearIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import type {
  GrandClass,
  GrandClassDefinition,
  Project,
  ProjectCatalog,
} from "../../types/project";
import { ProjectSelector } from "./ProjectSelector";

interface ProjectBarProps {
  projects: Project[];
  projectCatalog: ProjectCatalog;
  grandClassDefinitions?: GrandClassDefinition[];
  activeProjectId: string | null;
  disabled?: boolean;
  onProjectSelect: (id: string) => void;
  onCreateProject: (
    name: string,
    advancedMode?: boolean,
    grandClass?: GrandClass,
    groupId?: string | null,
  ) => void;
  onRenameProject: (id: string, name: string) => void;
  onDuplicateProject: (id: string, name: string) => void;
  onDeleteProject: (id: string) => void;
  onCreateProjectGroup: (name: string) => Promise<void>;
  onRenameProjectGroup: (groupId: string, name: string) => Promise<void>;
  onDeleteProjectGroup: (groupId: string) => Promise<void>;
  onMoveProjectToGroup: (projectId: string, groupId: string | null) => Promise<void>;
  onReorderProjectGroups: (groupIds: string[]) => Promise<void>;
  onReorderProjectsInGroup: (groupId: string | null, projectIds: string[]) => Promise<void>;
  onOpenProjectSettings: () => void;
}

type NameDialogMode = "create" | "rename" | "duplicate";

/**
 * Mimics the FGO formation `~ 1 ~ PARTY` ribbon: a thin banner with two
 * decorative chevrons flanking a centered pill that opens a dropdown
 * listing every project. Management actions live in the adjacent menu.
 *
 * Rendering keeps the bar visible across stages 1–3 so the user can
 * switch project without bouncing back to a sidebar. Project state
 * itself lives in `App.tsx`; this component is purely presentational.
 */
export function ProjectBar({
  projects,
  projectCatalog,
  grandClassDefinitions = [],
  activeProjectId,
  disabled = false,
  onProjectSelect,
  onCreateProject,
  onRenameProject,
  onDuplicateProject,
  onDeleteProject,
  onCreateProjectGroup,
  onRenameProjectGroup,
  onDeleteProjectGroup,
  onMoveProjectToGroup,
  onReorderProjectGroups,
  onReorderProjectsInGroup,
  onOpenProjectSettings,
}: ProjectBarProps) {
  const [nameDialogMode, setNameDialogMode] = useState<NameDialogMode | null>(null);
  const [draftName, setDraftName] = useState("");
  const [draftAdvancedMode, setDraftAdvancedMode] = useState(false);
  const [draftGrandClass, setDraftGrandClass] = useState<GrandClass>("saber");
  const [draftGroupId, setDraftGroupId] = useState<string | null>(null);
  const [dialogProjectId, setDialogProjectId] = useState<string | null>(null);
  const [deleteProjectId, setDeleteProjectId] = useState<string | null>(null);

  const activeProject =
    projects.find((p) => p.id === activeProjectId) ?? null;
  const dialogProject =
    projects.find((project) => project.id === dialogProjectId) ?? activeProject;
  const deleteProject =
    projects.find((project) => project.id === deleteProjectId) ?? null;
  const activeProjectGroupId =
    projectCatalog.groups.find((group) => group.projectIds.includes(activeProjectId ?? ""))?.id ??
    null;

  const trimmedDraftName = draftName.trim();
  const draftGrandDefinition = grandClassDefinitions.find(
    (definition) => definition.id === draftGrandClass,
  );
  const draftGrandSelection = draftGrandDefinition?.selectionGroup ?? draftGrandClass;
  const grandClassPrimaryOptions = useMemo(() => {
    const seenGroups = new Set<string>();
    return grandClassDefinitions.flatMap((definition) => {
      const group = definition.selectionGroup;
      if (!group) {
        return [{ value: definition.id, label: definition.label }];
      }
      if (seenGroups.has(group)) return [];
      seenGroups.add(group);
      return [{
        value: group,
        label: definition.selectionGroupLabel ?? definition.label,
      }];
    });
  }, [grandClassDefinitions]);
  const grandClassVariantOptions = draftGrandDefinition?.selectionGroup
    ? grandClassDefinitions.filter(
        (definition) => definition.selectionGroup === draftGrandDefinition.selectionGroup,
      )
    : [];

  const openNameDialog = useCallback(
    (mode: NameDialogMode, groupId?: string | null, projectId?: string) => {
      const project = projectId
        ? projects.find((item) => item.id === projectId) ?? null
        : activeProject;
      setNameDialogMode(mode);
      setDialogProjectId(project?.id ?? null);
      if (mode === "rename") {
        setDraftName(project?.name ?? "");
      } else if (mode === "duplicate") {
        setDraftName(project ? `${project.name} 副本` : "");
      } else {
        setDraftName(`队伍 ${projects.length + 1}`);
      }
      setDraftAdvancedMode(false);
      setDraftGrandClass(grandClassDefinitions[0]?.id ?? "saber");
      setDraftGroupId(
        mode === "create"
          ? groupId === undefined
            ? activeProjectGroupId
            : groupId
          : null,
      );
    },
    [activeProject, activeProjectGroupId, grandClassDefinitions, projects]
  );

  const handleNameSubmit = useCallback(
    (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (!nameDialogMode || !trimmedDraftName) return;
      if (nameDialogMode === "create") {
        onCreateProject(trimmedDraftName, draftAdvancedMode, draftGrandClass, draftGroupId);
      } else if (nameDialogMode === "rename" && dialogProject) {
        onRenameProject(dialogProject.id, trimmedDraftName);
      } else if (nameDialogMode === "duplicate" && dialogProject) {
        onDuplicateProject(dialogProject.id, trimmedDraftName);
      }
      setNameDialogMode(null);
      setDialogProjectId(null);
    },
    [
      dialogProject,
      draftAdvancedMode,
      draftGrandClass,
      draftGroupId,
      nameDialogMode,
      onCreateProject,
      onDuplicateProject,
      onRenameProject,
      trimmedDraftName,
    ]
  );

  const handleDeleteConfirm = useCallback(() => {
    if (deleteProject) {
      onDeleteProject(deleteProject.id);
    }
    setDeleteProjectId(null);
  }, [deleteProject, onDeleteProject]);

  const nameDialogTitle =
    nameDialogMode === "rename"
      ? "重命名队伍"
      : nameDialogMode === "duplicate"
        ? "复制队伍"
        : "新建队伍";
  const nameDialogAction =
    nameDialogMode === "rename"
      ? "保存"
      : nameDialogMode === "duplicate"
        ? "复制"
        : "新建";

  return (
    <Box className="project-bar">
      <Flex align="center" justify="center" gap="2" className="project-bar-controls">
        <ProjectSelector
          projects={projects}
          projectCatalog={projectCatalog}
          activeProjectId={activeProjectId}
          disabled={disabled}
          onProjectSelect={onProjectSelect}
          onRequestCreate={(groupId) => openNameDialog("create", groupId)}
          onRequestRename={(projectId) => openNameDialog("rename", undefined, projectId)}
          onRequestDelete={setDeleteProjectId}
          onCreateProjectGroup={onCreateProjectGroup}
          onRenameProjectGroup={onRenameProjectGroup}
          onDeleteProjectGroup={onDeleteProjectGroup}
          onMoveProjectToGroup={onMoveProjectToGroup}
          onReorderProjectGroups={onReorderProjectGroups}
          onReorderProjectsInGroup={onReorderProjectsInGroup}
        />

        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            <IconButton
              type="button"
              size="3"
              variant="surface"
              color="gray"
              aria-label="队伍操作"
              className="project-action-button"
              disabled={disabled}
            >
              <DotsHorizontalIcon width={16} height={16} />
            </IconButton>
          </DropdownMenu.Trigger>
          <DropdownMenu.Content>
            <DropdownMenu.Item
              disabled={disabled || !activeProject}
              onSelect={onOpenProjectSettings}
            >
              <Flex align="center" gap="2">
                <GearIcon width={12} height={12} />
                <Text size="2">队伍设置</Text>
              </Flex>
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item
              disabled={disabled || !activeProject}
              onSelect={() => openNameDialog("rename")}
            >
              <Flex align="center" gap="2">
                <Pencil1Icon width={12} height={12} />
                <Text size="2">重命名当前队伍</Text>
              </Flex>
            </DropdownMenu.Item>
            <DropdownMenu.Item
              disabled={disabled || !activeProject}
              onSelect={() => openNameDialog("duplicate")}
            >
              <Flex align="center" gap="2">
                <CopyIcon width={12} height={12} />
                <Text size="2">复制当前队伍</Text>
              </Flex>
            </DropdownMenu.Item>
            <DropdownMenu.Item
              color="red"
              disabled={disabled || !activeProject}
              onSelect={() => setDeleteProjectId(activeProject?.id ?? null)}
            >
              <Flex align="center" gap="2">
                <TrashIcon width={12} height={12} />
                <Text size="2">删除当前队伍</Text>
              </Flex>
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item disabled={disabled} onSelect={() => openNameDialog("create")}>
              <Flex align="center" gap="2">
                <PlusIcon width={12} height={12} />
                <Text size="2">新建队伍</Text>
              </Flex>
            </DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </Flex>

      <Dialog.Root
        open={nameDialogMode !== null}
        onOpenChange={(open) => {
          if (!open) {
            setNameDialogMode(null);
            setDialogProjectId(null);
          }
        }}
      >
        <Dialog.Content maxWidth="420px">
          <Dialog.Title size="4">{nameDialogTitle}</Dialog.Title>
          <form onSubmit={handleNameSubmit}>
            <Flex direction="column" gap="4">
              <label>
                <Text as="div" size="2" mb="2" weight="medium">
                  队伍名称
                </Text>
                <TextField.Root
                  value={draftName}
                  onChange={(event) => setDraftName(event.target.value)}
                  placeholder="输入队伍名称"
                  autoFocus
                />
              </label>
              {nameDialogMode === "create" && (
                <>
                  <Box>
                    <Text as="div" size="2" mb="2" weight="medium">
                      所属分组
                    </Text>
                    <Select.Root
                      value={draftGroupId ?? "__ungrouped__"}
                      onValueChange={(value) =>
                        setDraftGroupId(value === "__ungrouped__" ? null : value)
                      }
                    >
                      <Select.Trigger aria-label="所属分组" />
                      <Select.Content>
                        <Select.Item value="__ungrouped__">未分组</Select.Item>
                        {projectCatalog.groups.map((group) => (
                          <Select.Item key={group.id} value={group.id}>
                            {group.name}
                          </Select.Item>
                        ))}
                      </Select.Content>
                    </Select.Root>
                  </Box>
                  <Box>
                    <Text as="div" size="2" mb="2" weight="medium">
                      队伍模式
                    </Text>
                    <Select.Root
                      value={draftAdvancedMode ? "grand" : "normal"}
                      onValueChange={(value) => setDraftAdvancedMode(value === "grand")}
                    >
                      <Select.Trigger aria-label="队伍模式" />
                      <Select.Content>
                        <Select.Item value="normal">普通模式</Select.Item>
                        <Select.Item value="grand">戴冠战模式</Select.Item>
                      </Select.Content>
                    </Select.Root>
                  </Box>
                  {draftAdvancedMode && (
                    <Box>
                      <Text as="div" size="2" mb="2" weight="medium">
                        冠位职阶
                      </Text>
                      <Flex align="center" gap="2">
                        <Select.Root
                          value={draftGrandSelection}
                          onValueChange={(value) => {
                            const direct = grandClassDefinitions.find(
                              (definition) => definition.id === value,
                            );
                            const grouped = grandClassDefinitions.find(
                              (definition) => definition.selectionGroup === value,
                            );
                            setDraftGrandClass((direct ?? grouped)?.id ?? value);
                          }}
                        >
                          <Select.Trigger aria-label="冠位职阶" />
                          <Select.Content>
                            {grandClassPrimaryOptions.map((option) => (
                              <Select.Item key={option.value} value={option.value}>
                                {option.label}
                              </Select.Item>
                            ))}
                          </Select.Content>
                        </Select.Root>
                        {grandClassVariantOptions.length > 0 && (
                          <Select.Root
                            key={draftGrandDefinition?.selectionGroup}
                            value={draftGrandClass}
                            onValueChange={(value) => setDraftGrandClass(value as GrandClass)}
                          >
                            <Select.Trigger aria-label="副本属性" />
                            <Select.Content>
                              {grandClassVariantOptions.map((definition) => (
                                <Select.Item key={definition.id} value={definition.id}>
                                  {definition.selectionOptionLabel ?? definition.label}
                                </Select.Item>
                              ))}
                            </Select.Content>
                          </Select.Root>
                        )}
                      </Flex>
                    </Box>
                  )}
                </>
              )}
              <Flex justify="end" gap="2">
                <Dialog.Close>
                  <Button type="button" variant="soft" color="gray">
                    取消
                  </Button>
                </Dialog.Close>
                <Button type="submit" disabled={!trimmedDraftName}>
                  {nameDialogAction}
                </Button>
              </Flex>
            </Flex>
          </form>
        </Dialog.Content>
      </Dialog.Root>

      <AlertDialog.Root
        open={deleteProjectId !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteProjectId(null);
        }}
      >
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>删除队伍</AlertDialog.Title>
          <AlertDialog.Description size="2">
            确定删除队伍「{deleteProject?.name ?? ""}」吗？此操作无法撤销。
          </AlertDialog.Description>
          <Flex gap="2" justify="end" mt="4">
            <AlertDialog.Cancel>
              <Button type="button" variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button type="button" color="red" onClick={handleDeleteConfirm}>
                删除
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Box>
  );
}
