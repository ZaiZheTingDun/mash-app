import type { CapturePreview, CapturePreviewQuery } from "../../../types/packetCapture";

export function previewFixture(recordId = "a", tab: CapturePreviewQuery["tab"] = "overview"): CapturePreview {
  return {
    recordId, tab,
    profile: [{ label: "御主", value: "测试御主" }, { label: "好友码", value: "001234567" }, { label: "签名", value: null }],
    currencies: [{ id: "currency:qp", itemId: 1, name: "QP", quantity: 100000, category: "currency", icon: null, note: null }],
    fruits: [{ id: "item:100", itemId: 100, name: "黄金果实", quantity: 1234, category: "consumable", icon: null, note: null }],
    summary: { servantTypes: 1, servantInstances: 2, ceTypes: 1, ceInstances: 3, storageInstances: 1, presents: 4, unclassifiedInstances: 0 },
    classes: [{ id: 8, name: "盾兵" }],
    servants: tab === "servants" ? [
      { instanceId: "first", svtId: 800100, number: 1, name: "玛修", rarity: 4, classId: 8, className: "盾兵", icon: null, level: 100, npLevel: 5, skills: [10,10,10], bond: 10, inStorage: false, details: [{ label: "追加技能 2", value: "Lv.10" }] },
      { instanceId: "second", svtId: 800100, number: 1, name: "玛修", rarity: 4, classId: 8, className: "盾兵", icon: null, level: 1, npLevel: 1, skills: [1,null,null], bond: 10, inStorage: true, details: [{ label: "追加技能 2", value: null }] },
    ] : [],
    craftEssences: tab === "craftEssences" ? [{ svtId: 9400010, number: 1, name: "顽强", rarity: 1, icon: null, quantity: 2, summary: "Lv.50 满破 ×1；Lv.1 突破 0 · 保管室 ×1", instances: [
      { instanceId: "ce1", level: 50, limitCount: 4, limitBroken: true, inStorage: false, locked: true },
      { instanceId: "ce2", level: 1, limitCount: 0, limitBroken: false, inStorage: true, locked: false },
    ] }] : [],
    items: tab === "items" ? [{ id: "item:999", itemId: 999, name: "物品 #999", quantity: 7, category: "other", icon: null, note: null }] : [],
    total: tab === "overview" ? 0 : tab === "servants" ? 2 : 1,
    page: 0, pageCount: 1,
  };
}
