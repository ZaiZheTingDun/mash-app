import type { Project } from "./project";

export interface FormationShareCode {
  code: string;
  characters: number;
  copied: boolean;
}

export interface FormationShareMember {
  position: number;
  name: string;
  servantId: number | null;
  servantVariantKey: string | null;
  faceId: number | null;
  isSupport: boolean;
  craftEssences: string[];
  mlbRequired: boolean;
}

export interface FormationShareSection {
  title: string;
  lines: string[];
}

export interface FormationSharePreview {
  token: string;
  sourceName: string;
  targetName: string;
  advancedMode: boolean;
  members: FormationShareMember[];
  supportRequirements: string[];
  sections: FormationShareSection[];
  warnings: string[];
}

export interface FormationShareIntegration {
  beforeShare?: (projectId: string) => Promise<void>;
  onFormationImported?: (project: Project) => void;
}
