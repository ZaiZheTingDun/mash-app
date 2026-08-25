import { useMemo, useState } from "react";
import type { CSSProperties, FormEvent } from "react";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  pointerWithin,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  AlertDialog,
  Box,
  Button,
  Dialog,
  Flex,
  IconButton,
  Popover,
  Text,
  TextField,
} from "@radix-ui/themes";
import {
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  MagnifyingGlassIcon,
  Pencil1Icon,
  PlusIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import type { Project, ProjectCatalog, ProjectGroup } from "../../types/project";
import {
  projectCatalogDropAction,
  projectDragId,
  projectGroupDragId,
} from "./projectCatalogOrder";

const UNGROUPED_KEY = "__ungrouped__";

const projectCatalogCollisionDetection: CollisionDetection = (args) => {
  const pointerCollisions = pointerWithin(args);
  return pointerCollisions.length > 0 ? pointerCollisions : closestCenter(args);
};

interface ProjectSection {
  key: string;
  name: string;
  groupId: string | null;
  projects: Project[];
}

interface ProjectSelectorProps {
  projects: Project[];
  projectCatalog: ProjectCatalog;
  activeProjectId: string | null;
  disabled?: boolean;
  onProjectSelect: (id: string) => void;
  onRequestCreate: (groupId: string | null) => void;
  onCreateProjectGroup: (name: string) => Promise<void>;
  onRenameProjectGroup: (groupId: string, name: string) => Promise<void>;
  onDeleteProjectGroup: (groupId: string) => Promise<void>;
  onMoveProjectToGroup: (projectId: string, groupId: string | null) => Promise<void>;
  onReorderProjectGroups: (groupIds: string[]) => Promise<void>;
  onReorderProjectsInGroup: (groupId: string | null, projectIds: string[]) => Promise<void>;
}

function projectSections(projects: Project[], catalog: ProjectCatalog): ProjectSection[] {
  const byId = new Map(projects.map((project) => [project.id, project]));
  const seen = new Set<string>();
  const sectionForGroup = (group: ProjectGroup): ProjectSection => ({
    key: group.id,
    name: group.name,
    groupId: group.id,
    projects: group.projectIds.flatMap((id) => {
      const project = byId.get(id);
      if (!project || seen.has(id)) return [];
      seen.add(id);
      return [project];
    }),
  });
  const grouped = catalog.groups.map(sectionForGroup);
  const ungroupedProjects = catalog.ungroupedProjectIds.flatMap((id) => {
    const project = byId.get(id);
    if (!project || seen.has(id)) return [];
    seen.add(id);
    return [project];
  });
  for (const project of projects) {
    if (!seen.has(project.id)) ungroupedProjects.push(project);
  }
  return [
    {
      key: UNGROUPED_KEY,
      name: "未分组",
      groupId: null,
      projects: ungroupedProjects,
    },
    ...grouped,
  ];
}

function projectModeLabel(project: Project) {
  return project.advancedMode ? "戴冠战" : "普通";
}

function SortableProjectGroup({
  group,
  count,
  active,
  onSelect,
}: {
  group: ProjectGroup;
  count: number;
  active: boolean;
  onSelect: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging, isOver } =
    useSortable({
      id: projectGroupDragId(group.id),
    });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  } as CSSProperties;
  return (
    <button
      ref={setNodeRef}
      type="button"
      style={style}
      className="project-manager-group project-manager-sortable-group"
      data-active={active}
      data-dragging={isDragging}
      data-drop-target={isOver}
      onClick={onSelect}
      {...attributes}
      {...listeners}
    >
      <Text size="2">{group.name}</Text>
      <Text size="1" color="gray">{count}</Text>
    </button>
  );
}

function ProjectGroupDropTarget({
  name,
  count,
  active,
  onSelect,
}: {
  name: string;
  count: number;
  active: boolean;
  onSelect: () => void;
}) {
  const { isOver, setNodeRef } = useDroppable({ id: projectGroupDragId(null) });
  return (
    <button
      ref={setNodeRef}
      type="button"
      className="project-manager-group"
      data-active={active}
      data-drop-target={isOver}
      onClick={onSelect}
    >
      <Text size="2">{name}</Text>
      <Text size="1" color="gray">{count}</Text>
    </button>
  );
}

function SortableProject({
  project,
}: {
  project: Project;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: projectDragId(project.id),
  });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  } as CSSProperties;
  return (
    <button
      ref={setNodeRef}
      type="button"
      style={style}
      className="project-manager-project"
      data-dragging={isDragging}
      {...attributes}
      {...listeners}
    >
      <Box className="project-manager-project-copy">
        <Text as="div" size="2" weight="medium">{project.name}</Text>
        <Text as="div" size="1" color="gray">{projectModeLabel(project)}</Text>
      </Box>
    </button>
  );
}

export function ProjectSelector({
  projects,
  projectCatalog,
  activeProjectId,
  disabled = false,
  onProjectSelect,
  onRequestCreate,
  onCreateProjectGroup,
  onRenameProjectGroup,
  onDeleteProjectGroup,
  onMoveProjectToGroup,
  onReorderProjectGroups,
  onReorderProjectsInGroup,
}: ProjectSelectorProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([UNGROUPED_KEY]));
  const [managerOpen, setManagerOpen] = useState(false);
  const [managedSectionKey, setManagedSectionKey] = useState(UNGROUPED_KEY);
  const [groupEditor, setGroupEditor] = useState<"create" | "rename" | null>(null);
  const [groupDraft, setGroupDraft] = useState("");
  const [groupError, setGroupError] = useState<string | null>(null);
  const [groupBusy, setGroupBusy] = useState(false);
  const [deleteGroupId, setDeleteGroupId] = useState<string | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const sections = useMemo(
    () => projectSections(projects, projectCatalog),
    [projectCatalog, projects],
  );
  const activeProject = projects.find((project) => project.id === activeProjectId) ?? null;
  const activeSection = sections.find((section) =>
    section.projects.some((project) => project.id === activeProjectId),
  );
  const managedSection =
    sections.find((section) => section.key === managedSectionKey) ?? sections[0];
  const deleteGroup = projectCatalog.groups.find((group) => group.id === deleteGroupId) ?? null;
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const searchResults = normalizedQuery
    ? sections.flatMap((section) =>
        section.projects
          .filter((project) => project.name.toLocaleLowerCase().includes(normalizedQuery))
          .map((project) => ({ project, sectionName: section.name })),
      )
    : [];

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen);
    if (nextOpen && activeSection) {
      setExpanded((current) => new Set([...current, activeSection.key]));
    }
    if (!nextOpen) setQuery("");
  };

  const selectProject = (projectId: string) => {
    onProjectSelect(projectId);
    setOpen(false);
  };

  const toggleSection = (key: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const openGroupEditor = (mode: "create" | "rename") => {
    const group = projectCatalog.groups.find((item) => item.id === managedSection?.groupId);
    setGroupEditor(mode);
    setGroupDraft(mode === "rename" ? group?.name ?? "" : "");
    setGroupError(null);
  };

  const submitGroup = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = groupDraft.trim();
    if (!name || !groupEditor) return;
    setGroupBusy(true);
    setGroupError(null);
    try {
      if (groupEditor === "create") {
        await onCreateProjectGroup(name);
      } else if (managedSection?.groupId) {
        await onRenameProjectGroup(managedSection.groupId, name);
      }
      setGroupEditor(null);
    } catch (error) {
      setGroupError(String(error));
    } finally {
      setGroupBusy(false);
    }
  };

  const confirmDeleteGroup = async () => {
    if (!deleteGroupId) return;
    try {
      await onDeleteProjectGroup(deleteGroupId);
      setManagedSectionKey(UNGROUPED_KEY);
      setDeleteGroupId(null);
    } catch (error) {
      console.error(error);
    }
  };

  const handleCatalogDragEnd = ({ active, over }: DragEndEvent) => {
    if (!managedSection || !over || active.id === over.id) return;
    const action = projectCatalogDropAction(
      active.id,
      over.id,
      managedSection.groupId,
      projectCatalog.groups.map((group) => group.id),
      managedSection.projects.map((project) => project.id),
    );
    if (action.type === "reorderGroups") {
      void onReorderProjectGroups(action.groupIds);
    } else if (action.type === "reorderProjects") {
      void onReorderProjectsInGroup(action.groupId, action.projectIds);
    } else if (action.type === "moveProject") {
      void onMoveProjectToGroup(action.projectId, action.groupId);
    }
  };

  const triggerLabel = activeProject?.name ?? "选择队伍";

  return (
    <>
      <Popover.Root open={open} onOpenChange={handleOpenChange}>
        <Popover.Trigger>
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
        </Popover.Trigger>
        <Popover.Content className="project-selector-popover" width="360px">
          <TextField.Root
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索队伍"
            aria-label="搜索队伍"
            autoFocus
          >
            <TextField.Slot>
              <MagnifyingGlassIcon width={15} height={15} />
            </TextField.Slot>
          </TextField.Root>

          <Box className="project-selector-list">
            {projects.length === 0 ? (
              <Flex align="center" justify="center" py="5">
                <Text size="2" color="gray">暂无队伍</Text>
              </Flex>
            ) : normalizedQuery ? (
              searchResults.length > 0 ? (
                searchResults.map(({ project, sectionName }) => (
                  <button
                    type="button"
                    key={project.id}
                    className="project-selector-project"
                    data-active={project.id === activeProjectId}
                    onClick={() => selectProject(project.id)}
                  >
                    <span className="project-selector-check">
                      {project.id === activeProjectId && <CheckIcon width={14} height={14} />}
                    </span>
                    <span className="project-selector-project-copy">
                      <Text size="2" weight="medium">{project.name}</Text>
                      <Text size="1" color="gray">位于：{sectionName}</Text>
                    </span>
                    <Text size="1" color="gray" className="project-selector-mode">
                      {projectModeLabel(project)}
                    </Text>
                  </button>
                ))
              ) : (
                <Flex align="center" justify="center" py="5">
                  <Text size="2" color="gray">没有匹配的队伍</Text>
                </Flex>
              )
            ) : (
              sections.map((section) => {
                const isExpanded = expanded.has(section.key);
                return (
                  <Box key={section.key} className="project-selector-section">
                    <button
                      type="button"
                      className="project-selector-section-trigger"
                      aria-expanded={isExpanded}
                      onClick={() => toggleSection(section.key)}
                    >
                      {isExpanded ? (
                        <ChevronDownIcon width={14} height={14} />
                      ) : (
                        <ChevronRightIcon width={14} height={14} />
                      )}
                      <Text size="2" weight="medium">{section.name}</Text>
                      <Text size="1" color="gray" className="project-selector-count">
                        {section.projects.length}
                      </Text>
                    </button>
                    {isExpanded && (
                      <Box className="project-selector-section-projects">
                        {section.projects.length === 0 ? (
                          <Text size="1" color="gray" className="project-selector-empty-group">
                            暂无队伍
                          </Text>
                        ) : (
                          section.projects.map((project) => (
                            <button
                              type="button"
                              key={project.id}
                              className="project-selector-project"
                              data-active={project.id === activeProjectId}
                              onClick={() => selectProject(project.id)}
                            >
                              <span className="project-selector-check">
                                {project.id === activeProjectId && (
                                  <CheckIcon width={14} height={14} />
                                )}
                              </span>
                              <Text size="2" className="project-selector-project-name">
                                {project.name}
                              </Text>
                              <Text size="1" color="gray" className="project-selector-mode">
                                {projectModeLabel(project)}
                              </Text>
                            </button>
                          ))
                        )}
                      </Box>
                    )}
                  </Box>
                );
              })
            )}
          </Box>

          <Flex justify="between" align="center" className="project-selector-footer">
            <Button
              type="button"
              size="1"
              variant="ghost"
              onClick={() => {
                onRequestCreate(activeSection?.groupId ?? null);
                setOpen(false);
              }}
            >
              <PlusIcon width={13} height={13} />
              新建队伍
            </Button>
            <Button
              type="button"
              size="1"
              variant="ghost"
              color="gray"
              onClick={() => {
                setManagedSectionKey(activeSection?.key ?? UNGROUPED_KEY);
                setManagerOpen(true);
                setOpen(false);
              }}
            >
              管理所有队伍…
            </Button>
          </Flex>
        </Popover.Content>
      </Popover.Root>

      <Dialog.Root open={managerOpen} onOpenChange={setManagerOpen}>
        <Dialog.Content maxWidth="760px" className="project-manager-dialog">
          <Dialog.Title size="4">管理所有队伍</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            拖动调整顺序，也可以把队伍直接拖到左侧分组。
          </Dialog.Description>
          <DndContext
            sensors={sensors}
            collisionDetection={projectCatalogCollisionDetection}
            onDragEnd={handleCatalogDragEnd}
          >
            <Flex className="project-manager-body">
            <Flex direction="column" className="project-manager-sidebar">
              <Flex align="center" justify="between" className="project-manager-heading">
                <Text size="2" weight="bold">分组</Text>
                <IconButton
                  type="button"
                  size="1"
                  variant="ghost"
                  aria-label="新建分组"
                  onClick={() => openGroupEditor("create")}
                >
                  <PlusIcon />
                </IconButton>
              </Flex>
              <ProjectGroupDropTarget
                name="未分组"
                count={sections[0]?.projects.length ?? 0}
                active={managedSection?.key === UNGROUPED_KEY}
                onSelect={() => setManagedSectionKey(UNGROUPED_KEY)}
              />
              <SortableContext
                items={projectCatalog.groups.map((group) => projectGroupDragId(group.id))}
                strategy={verticalListSortingStrategy}
              >
                {projectCatalog.groups.map((group) => {
                  const section = sections.find((item) => item.groupId === group.id);
                  return (
                    <SortableProjectGroup
                      key={group.id}
                      group={group}
                      count={section?.projects.length ?? 0}
                      active={managedSection?.groupId === group.id}
                      onSelect={() => setManagedSectionKey(group.id)}
                    />
                  );
                })}
              </SortableContext>
            </Flex>

            <Flex direction="column" className="project-manager-content">
              <Flex align="center" justify="between" className="project-manager-heading">
                <Box>
                  <Text as="div" size="3" weight="bold">{managedSection?.name ?? "未分组"}</Text>
                  <Text as="div" size="1" color="gray">
                    {managedSection?.projects.length ?? 0} 支队伍
                  </Text>
                </Box>
                {managedSection?.groupId && (
                  <Flex gap="1">
                    <IconButton
                      type="button"
                      size="2"
                      variant="soft"
                      color="gray"
                      aria-label="重命名分组"
                      onClick={() => openGroupEditor("rename")}
                    >
                      <Pencil1Icon />
                    </IconButton>
                    <IconButton
                      type="button"
                      size="2"
                      variant="soft"
                      color="red"
                      aria-label="删除分组"
                      onClick={() => setDeleteGroupId(managedSection.groupId)}
                    >
                      <TrashIcon />
                    </IconButton>
                  </Flex>
                )}
              </Flex>
              <Box className="project-manager-projects">
                {(managedSection?.projects.length ?? 0) === 0 ? (
                  <Flex align="center" justify="center" py="6">
                    <Text size="2" color="gray">这个分组中暂无队伍</Text>
                  </Flex>
                ) : (
                    <SortableContext
                      items={managedSection?.projects.map((project) => projectDragId(project.id)) ?? []}
                      strategy={verticalListSortingStrategy}
                    >
                      {managedSection?.projects.map((project) => (
                        <SortableProject
                          key={project.id}
                          project={project}
                        />
                      ))}
                    </SortableContext>
                )}
              </Box>
            </Flex>
            </Flex>
          </DndContext>
          <Flex justify="end" mt="4">
            <Dialog.Close>
              <Button type="button" variant="soft" color="gray">完成</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <Dialog.Root
        open={groupEditor !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setGroupEditor(null);
        }}
      >
        <Dialog.Content maxWidth="400px">
          <Dialog.Title>{groupEditor === "rename" ? "重命名分组" : "新建分组"}</Dialog.Title>
          <form onSubmit={submitGroup}>
            <Flex direction="column" gap="3">
              <label>
                <Text as="div" size="2" mb="2" weight="medium">分组名称</Text>
                <TextField.Root
                  value={groupDraft}
                  onChange={(event) => setGroupDraft(event.target.value)}
                  placeholder="输入分组名称"
                  autoFocus
                />
              </label>
              {groupError && <Text size="1" color="red">{groupError}</Text>}
              <Flex justify="end" gap="2">
                <Dialog.Close>
                  <Button type="button" variant="soft" color="gray">取消</Button>
                </Dialog.Close>
                <Button type="submit" disabled={!groupDraft.trim() || groupBusy}>
                  {groupEditor === "rename" ? "保存" : "新建"}
                </Button>
              </Flex>
            </Flex>
          </form>
        </Dialog.Content>
      </Dialog.Root>

      <AlertDialog.Root
        open={deleteGroupId !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setDeleteGroupId(null);
        }}
      >
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>删除分组</AlertDialog.Title>
          <AlertDialog.Description size="2">
            删除分组「{deleteGroup?.name ?? ""}」后，其中的队伍会移到“未分组”，不会删除队伍配置。
          </AlertDialog.Description>
          <Flex gap="2" justify="end" mt="4">
            <AlertDialog.Cancel>
              <Button type="button" variant="soft" color="gray">取消</Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button type="button" color="red" onClick={() => void confirmDeleteGroup()}>
                删除分组
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </>
  );
}
