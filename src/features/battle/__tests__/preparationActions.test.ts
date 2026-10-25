import { describe, expect, it } from "vitest";
import {
  makePreparationAction, makeOrderChangeAction, startPreparationSkill, selectPreparationSkillOption,
  type PreparationContext, type PreparationResult,
} from "../preparationActions";
import type { PartySlot, PrepSource } from "../battleSceneModel";
import type { PreparationAction } from "../../../types/command";

const context: PreparationContext = {
  members: Array.from({ length: 6 }, (_, index) => ({
    memberId: `member-${index}`, isSupport: index === 2,
    servant: { id: index === 2 ? 1 : index + 1, variantKey: `${index}:1`, name_cn: "", name_jp: "", name_en: "", class: "saber", rarity: 5 },
  })),
  mysticCode: { id: 1, name: "", itemMalePath: null, itemFemalePath: null, masterFigureMalePath: null, masterFigureFemalePath: null, masterFaceMalePath: null, masterFaceFemalePath: null,
    skills: [{ slot: 1, id: 1, name: "", iconPath: null, targetingMode: "needsTarget" }, { slot: 2, id: 2, name: "", iconPath: null, targetingMode: "noTarget" }, { slot: 3, id: 3, name: "", iconPath: null, targetingMode: "orderChange" }] },
  disableAutoSkillTargetRecognition: false,
  skillTargetStatus: () => "needsTarget",
  skillSelection: () => null,
};
const sources: PrepSource[] = ["servant_1", "equipment"];
function actionData(action: PreparationAction) {
  const { id, ...data } = action;
  expect(id).toEqual(expect.any(String));
  return data;
}
function completeTarget(result: PreparationResult, target: PartySlot) {
  if (result.type !== "draft" || result.draft.step !== "target") throw Error("expected target draft");
  return makePreparationAction(result.draft, target, context.members);
}

describe("shared preparation actions", () => {
  it.each(sources)("guided and quick entry produce identical %s actions", source => {
    const guided = startPreparationSkill(context, source, "skill_1");
    const simple = startPreparationSkill(context, source, "skill_1", "servant_3");
    expect(guided).toEqual({ type: "draft", draft: { step: "target", source, option: "skill_1", allowNoTarget: false } });
    if (simple.type !== "action") throw Error("expected quick action");
    expect(actionData(completeTarget(guided, "servant_3"))).toEqual(actionData(simple.action));
    expect(simple.action).toEqual(expect.objectContaining({ target: "servant_3", targetMemberId: "member-2", targetServantId: 1, targetIsSupport: true }));
  });

  it.each(sources)("requires a %s target unless automatic recognition is disabled", source => {
    expect(startPreparationSkill(context, source, "skill_1", null)).toEqual(expect.objectContaining({ type: "draft", draft: expect.objectContaining({ allowNoTarget: false }) }));
    const result = startPreparationSkill({ ...context, disableAutoSkillTargetRecognition: true }, source, "skill_1", null);
    expect(result).toEqual(expect.objectContaining({ type: "action", action: expect.objectContaining({ target: null }) }));
    // Guided entry still offers manual target choice when recognition is off.
    expect(startPreparationSkill({ ...context, disableAutoSkillTargetRecognition: true }, source, "skill_1")).toEqual({ type: "draft", draft: { step: "target", source, option: "skill_1" } });
  });

  it.each(sources)("discards a selected target for no-target %s skills", source => {
    const ctx: PreparationContext = { ...context, skillTargetStatus: () => "noTarget" };
    const skill = source === "equipment" ? "skill_2" : "skill_1";
    const guided = startPreparationSkill(ctx, source, skill);
    const simple = startPreparationSkill(ctx, source, skill, "servant_3");
    if (guided.type !== "action" || simple.type !== "action") throw Error("expected immediate actions");
    expect(actionData(guided.action)).toEqual(actionData(simple.action));
    expect(simple.action).toEqual(expect.objectContaining({ target: null, targetMemberId: null, targetServantId: null, targetIsSupport: false }));
  });

  it.each(["mixed", "unknown"] as const)("allows an unset quick target for %s while guided entry offers a choice", status => {
    const ctx: PreparationContext = { ...context, skillTargetStatus: () => status };
    expect(startPreparationSkill(ctx, "servant_1", "skill_1", null).type).toBe("action");
    expect(startPreparationSkill(ctx, "servant_1", "skill_1")).toEqual({ type: "draft", draft: { step: "target", source: "servant_1", option: "skill_1", ...(status === "mixed" ? { allowNoTarget: true } : {}) } });
  });

  it("uses one skill-selection flow for both layouts, including advanced backline sources", () => {
    const selection = { servantCollectionNo: 6, skillId: 4, skillNum: 1, selectionType: "SelectAddInfo" as const, supplementaryTypes: [], options: [{ index: 1, label: "乙" }, { index: 2, label: "丙" }] };
    const ctx: PreparationContext = { ...context, skillSelection: () => selection };
    const result = startPreparationSkill(ctx, "servant_6", "skill_1", "servant_3");
    if (result.type !== "draft" || result.draft.step !== "skillSelection") throw Error("expected selection");
    const guided = selectPreparationSkillOption(ctx, result.draft, selection.options[1]);
    const simple = selectPreparationSkillOption(ctx, result.draft, selection.options[1], "servant_3");
    if (simple.type !== "action") throw Error("expected quick action");
    expect(actionData(completeTarget(guided, "servant_3"))).toEqual(actionData(simple.action));
    expect(simple.action).toEqual(expect.objectContaining({ servantMemberId: "member-5", servantId: 6, skillSelection: { type: "SelectAddInfo", index: 2, optionCount: 2, label: "丙" } }));
  });

  it("uses the same order-change draft and preserves front/back member identity", () => {
    const guided = startPreparationSkill(context, "equipment", "skill_3");
    const simple = startPreparationSkill(context, "equipment", "skill_3", "servant_1");
    expect(guided).toEqual(simple);
    if (guided.type !== "draft" || guided.draft.step !== "orderChange") throw Error("expected order change");
    expect(makeOrderChangeAction(guided.draft, "servant_4", context.members)).toBeNull();
    const action = makeOrderChangeAction({ ...guided.draft, front: "servant_3" }, "servant_4", context.members);
    expect(action).toEqual(expect.objectContaining({ target: null, orderChange: {
      front: "servant_3", frontMemberId: "member-2", frontServantId: 1, frontIsSupport: true,
      back: "servant_4", backMemberId: "member-3", backServantId: 4, backIsSupport: false,
    } }));
    // Preserve guided entry's existing behavior when auto recognition is disabled.
    expect(startPreparationSkill({ ...context, disableAutoSkillTargetRecognition: true }, "equipment", "skill_3").type).toBe("draft");
    expect(startPreparationSkill({ ...context, disableAutoSkillTargetRecognition: true }, "equipment", "skill_3", null).type).toBe("action");
  });

  it("shares seal and enemy-target action construction without changing their wire formats", () => {
    expect(startPreparationSkill(context, "commandSpell", "restore", null)).toEqual({ type: "draft", draft: { step: "target", source: "commandSpell", option: "restore", allowNoTarget: false } });
    expect(startPreparationSkill(context, "commandSpell", "np_release", "servant_3")).toEqual({ type: "action", action: expect.objectContaining({ type: "commandSpell", spell: "np_release", targetMemberId: "member-2", targetIsSupport: true }) });
    expect(actionData(makePreparationAction({ step: "target", source: "enemyTarget", option: "select" }, "enemy_6", context.members))).toEqual({ type: "enemyTarget", target: "enemy_6" });
  });
});
