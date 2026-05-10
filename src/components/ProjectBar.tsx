import { useCallback, useState } from "react";
import type { FormEvent } from "react";
import {
  AlertDialog,
  Box,
  Button,
  Dialog,
  DropdownMenu,
  Flex,
  IconButton,
  Text,
  TextField,
} from "@radix-ui/themes";
import {
  ChevronDownIcon,
  CopyIcon,
  DotsHorizontalIcon,
  Pencil1Icon,
  PlusIcon,
  TrashIcon,
  CheckIcon,
} from "@radix-ui/react-icons";
import type { Project } from "../types/project";

interface ProjectBarProps {
  projects: Project[];
  activeProjectId: string | null;
  disabled?: boolean;
  onProjectSelect: (id: string) => void;
  onCreateProject: (name: string) => void;
  onRenameProject: (id: string, name: string) => void;
  onDuplicateProject: (id: string, name: string) => void;
  onDeleteProject: (id: string) => void;
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
  activeProjectId,
  disabled = false,
  onProjectSelect,
  onCreateProject,
  onRenameProject,
  onDuplicateProject,
  onDeleteProject,
}: ProjectBarProps) {
  const [nameDialogMode, setNameDialogMode] = useState<NameDialogMode | null>(null);
  const [draftName, setDraftName] = useState("");
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);

  const activeProject =
    projects.find((p) => p.id === activeProjectId) ?? null;

  const triggerLabel = activeProject?.name ?? "选择队伍";
  const trimmedDraftName = draftName.trim();

  const openNameDialog = useCallback(
    (mode: NameDialogMode) => {
      setNameDialogMode(mode);
      if (mode === "rename") {
        setDraftName(activeProject?.name ?? "");
      } else if (mode === "duplicate") {
        setDraftName(activeProject ? `${activeProject.name} 副本` : "");
      } else {
        setDraftName(`队伍 ${projects.length + 1}`);
      }
    },
    [activeProject, projects.length]
  );

  const handleNameSubmit = useCallback(
    (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (!nameDialogMode || !trimmedDraftName) return;
      if (nameDialogMode === "create") {
        onCreateProject(trimmedDraftName);
      } else if (nameDialogMode === "rename" && activeProject) {
        onRenameProject(activeProject.id, trimmedDraftName);
      } else if (nameDialogMode === "duplicate" && activeProject) {
        onDuplicateProject(activeProject.id, trimmedDraftName);
      }
      setNameDialogMode(null);
    },
    [
      activeProject,
      nameDialogMode,
      onCreateProject,
      onDuplicateProject,
      onRenameProject,
      trimmedDraftName,
    ]
  );

  const handleDeleteConfirm = useCallback(() => {
    if (activeProject) {
      onDeleteProject(activeProject.id);
    }
    setDeleteConfirmOpen(false);
  }, [activeProject, onDeleteProject]);

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
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            <Button
              type="button"
              size="3"
              variant="surface"
              color="gray"
              className="project-bar-pill"
              disabled={disabled}
            >
              <Text size="3" weight="bold" className="project-bar-title">
                {`～ ${triggerLabel} ～`}
              </Text>
              <ChevronDownIcon width={15} height={15} className="project-bar-menu-icon" />
            </Button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Content className="project-select-menu">
            {projects.length === 0 ? (
              <DropdownMenu.Item disabled>
                <Text size="2" color="gray">暂无队伍</Text>
              </DropdownMenu.Item>
            ) : (
              projects.map((p) => {
                const isActive = p.id === activeProjectId;
                return (
                  <DropdownMenu.Item
                    key={p.id}
                    disabled={disabled}
                    onSelect={() => onProjectSelect(p.id)}
                  >
                    <Flex align="center" gap="2" justify="between" width="100%">
                      <Text size="2">{p.name}</Text>
                      {isActive ? (
                        <CheckIcon width={14} height={14} />
                      ) : (
                        <span className="project-bar-check-spacer" />
                      )}
                    </Flex>
                  </DropdownMenu.Item>
                );
              })
            )}
          </DropdownMenu.Content>
        </DropdownMenu.Root>

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
              onSelect={() => setDeleteConfirmOpen(true)}
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
          if (!open) setNameDialogMode(null);
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

      <AlertDialog.Root open={deleteConfirmOpen} onOpenChange={setDeleteConfirmOpen}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>删除队伍</AlertDialog.Title>
          <AlertDialog.Description size="2">
            确定删除队伍「{activeProject?.name ?? ""}」吗？此操作无法撤销。
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
