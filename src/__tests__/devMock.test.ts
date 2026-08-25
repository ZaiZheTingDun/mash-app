import { describe, expect, it } from "vitest";
import { invokeDevMock } from "../devMock";
import type { Project, ProjectCatalog } from "../types/project";

describe("invokeDevMock project catalog", () => {
  it("supports the complete project grouping workflow", async () => {
    const initialProjects = await invokeDevMock<Project[]>("list_projects");
    const initialProjectId = initialProjects[0].id;
    let catalog = await invokeDevMock<ProjectCatalog>("get_project_catalog");
    expect(catalog.ungroupedProjectIds).toContain(initialProjectId);

    catalog = await invokeDevMock<ProjectCatalog>("create_project_group", { name: "周回" });
    const groupId = catalog.groups[0].id;
    catalog = await invokeDevMock<ProjectCatalog>("create_project_group", { name: "高难" });
    const challengeGroupId = catalog.groups.find((group) => group.name === "高难")!.id;
    catalog = await invokeDevMock<ProjectCatalog>("reorder_project_groups", {
      groupIds: [challengeGroupId, groupId],
    });
    expect(catalog.groups.map((group) => group.id)).toEqual([challengeGroupId, groupId]);

    catalog = await invokeDevMock<ProjectCatalog>("move_project_to_group", {
      projectId: initialProjectId,
      groupId,
    });
    expect(catalog.groups.find((group) => group.id === groupId)?.projectIds).toEqual([
      initialProjectId,
    ]);

    const created = await invokeDevMock<Project>("create_project", {
      name: "模拟周回队伍",
      groupId,
    });
    const duplicated = await invokeDevMock<Project>("duplicate_project", {
      sourceId: created.id,
      name: "模拟周回队伍副本",
    });
    catalog = await invokeDevMock<ProjectCatalog>("get_project_catalog");
    expect(catalog.groups.find((group) => group.id === groupId)?.projectIds).toEqual([
      initialProjectId,
      created.id,
      duplicated.id,
    ]);

    catalog = await invokeDevMock<ProjectCatalog>("reorder_projects_in_group", {
      groupId,
      projectIds: [duplicated.id, created.id, initialProjectId],
    });
    expect(catalog.groups.find((group) => group.id === groupId)?.projectIds).toEqual([
      duplicated.id,
      created.id,
      initialProjectId,
    ]);

    catalog = await invokeDevMock<ProjectCatalog>("rename_project_group", {
      groupId,
      name: "活动周回",
    });
    expect(catalog.groups.find((group) => group.id === groupId)?.name).toBe("活动周回");

    await invokeDevMock<ProjectCatalog>("delete_project_group", {
      groupId: challengeGroupId,
    });
    catalog = await invokeDevMock<ProjectCatalog>("delete_project_group", { groupId });
    expect(catalog.groups).toEqual([]);
    expect(catalog.ungroupedProjectIds).toEqual(
      expect.arrayContaining([initialProjectId, created.id, duplicated.id]),
    );

    await invokeDevMock("delete_project", { id: created.id });
    await invokeDevMock("delete_project", { id: duplicated.id });
    catalog = await invokeDevMock<ProjectCatalog>("get_project_catalog");
    expect(catalog.ungroupedProjectIds).not.toContain(created.id);
    expect(catalog.ungroupedProjectIds).not.toContain(duplicated.id);
  });
});
