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
}

export interface FormationSharePreview {
  token: string;
  targetName: string;
  members: FormationShareMember[];
}
