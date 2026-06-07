export interface ServantNameAlias {
  nameJp?: string | null;
  nameCn?: string | null;
}

export interface Servant {
  id: number;
  variantKey: string;
  faceId?: number | null;
  name_cn: string;
  name_cn_server?: string;
  name_jp: string;
  name_en: string;
  name_other?: string;
  overWriteServantNames?: ServantNameAlias[];
  class: string;
  rarity: number;
  noblePhantasmName?: string | null;
  noblePhantasmCard?: "buster" | "arts" | "quick" | null;
}
