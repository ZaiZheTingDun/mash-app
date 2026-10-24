import { invoke } from "../../tauri";
import type { FormationShareCode, FormationSharePreview } from "../../types/formationShare";
import type { Project } from "../../types/project";

export const formationShareApi = {
  create: (projectId: string) => invoke<FormationShareCode>("create_formation_share", { projectId }),
  preview: (text: string) => invoke<FormationSharePreview>("preview_formation_share", { text }),
  discard: (previewToken: string) => invoke<void>("discard_formation_share_preview", { previewToken }),
  import: (previewToken: string, name: string, groupId: string | null) =>
    invoke<Project>("import_formation_share", { previewToken, name, groupId }),
};
