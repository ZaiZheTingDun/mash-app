import { arrayMove } from "@dnd-kit/sortable";

export function reorderProjectCatalogIds(
  ids: string[],
  activeId: string | number,
  overId: string | number,
) {
  const oldIndex = ids.findIndex((id) => id === activeId);
  const newIndex = ids.findIndex((id) => id === overId);
  if (oldIndex < 0 || newIndex < 0 || oldIndex === newIndex) return ids;
  return arrayMove(ids, oldIndex, newIndex);
}

const GROUP_PREFIX = "group:";
const PROJECT_PREFIX = "project:";
const UNGROUPED_ID = "__ungrouped__";

export function projectGroupDragId(groupId: string | null) {
  return `${GROUP_PREFIX}${groupId ?? UNGROUPED_ID}`;
}

export function projectDragId(projectId: string) {
  return `${PROJECT_PREFIX}${projectId}`;
}

export type ProjectCatalogDropAction =
  | { type: "reorderGroups"; groupIds: string[] }
  | { type: "reorderProjects"; groupId: string | null; projectIds: string[] }
  | { type: "moveProject"; projectId: string; groupId: string | null }
  | { type: "none" };

export function projectCatalogDropAction(
  activeDragId: string | number,
  overDragId: string | number,
  currentGroupId: string | null,
  groupIds: string[],
  projectIds: string[],
): ProjectCatalogDropAction {
  const activeId = String(activeDragId);
  const overId = String(overDragId);
  if (activeId.startsWith(PROJECT_PREFIX)) {
    const projectId = activeId.slice(PROJECT_PREFIX.length);
    if (overId.startsWith(GROUP_PREFIX)) {
      const target = overId.slice(GROUP_PREFIX.length);
      return {
        type: "moveProject",
        projectId,
        groupId: target === UNGROUPED_ID ? null : target,
      };
    }
    if (overId.startsWith(PROJECT_PREFIX)) {
      const reordered = reorderProjectCatalogIds(
        projectIds,
        projectId,
        overId.slice(PROJECT_PREFIX.length),
      );
      return reordered === projectIds
        ? { type: "none" }
        : { type: "reorderProjects", groupId: currentGroupId, projectIds: reordered };
    }
  }
  if (activeId.startsWith(GROUP_PREFIX) && overId.startsWith(GROUP_PREFIX)) {
    const activeGroupId = activeId.slice(GROUP_PREFIX.length);
    const overGroupId = overId.slice(GROUP_PREFIX.length);
    if (overGroupId === UNGROUPED_ID) return { type: "none" };
    const reordered = reorderProjectCatalogIds(groupIds, activeGroupId, overGroupId);
    return reordered === groupIds
      ? { type: "none" }
      : { type: "reorderGroups", groupIds: reordered };
  }
  return { type: "none" };
}
