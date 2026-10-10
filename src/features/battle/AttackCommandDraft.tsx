import { AttackCardOptionButtons } from "../../components/common/AttackCardOptionButtons";
import { CommandDraftHeading } from "../../components/common/CommandDraftHeading";
import { ServantChoice } from "../../components/common/ServantChoice";
import { ATTACK_OPTIONS, sourceIndex, type AttackDraft, type AttackSource } from "./battleSceneModel";
import type { PartyMember } from "../team/partyServants";

type AttackOption = typeof ATTACK_OPTIONS[number]["value"];

export function AttackCommandDraft({ draft, members, faces, onDraftChange, onSelect, onCancel }: {
  draft: AttackDraft;
  members: PartyMember[];
  faces: Record<string, string | null>;
  onDraftChange: (draft: AttackDraft) => void;
  onSelect: (source: AttackSource, option: AttackOption) => void;
  onCancel: () => void;
}) {
  const reselect = () => onDraftChange({ step: "source", targetIndex: draft.targetIndex });
  const choice = (index: number, selected: boolean) => {
    const member = members[index] ?? { servant: null, isSupport: false };
    return <ServantChoice key={index} servant={member.servant} index={index}
      src={member.servant ? faces[member.servant.variantKey] : null} isSupport={member.isSupport} selected={selected}
      onClick={selected ? reselect : () => onDraftChange({ step: "option", source: `servant_${index + 1}` as AttackSource, targetIndex: draft.targetIndex })} />;
  };
  return <div className="battle-add-row command-inline-draft command-attack-draft">
    <CommandDraftHeading title={draft.targetIndex == null ? "添加攻击指令" : "设置攻击"}
      hint={draft.step === "source" ? "选择前排从者" : "选择攻击类型"}
      onReselect={draft.step === "option" ? reselect : undefined} onCancel={onCancel} />
    <div className="battle-choice-row">
      {draft.step === "source" ? members.slice(0, 3).map((_, index) => choice(index, false)) : <>
        {choice(sourceIndex(draft.source) ?? 0, true)}
        <AttackCardOptionButtons label="攻击类型" className="command-attack-options"
          options={ATTACK_OPTIONS.map((option, iconIndex) => ({ ...option, iconIndex }))}
          onSelect={option => onSelect(draft.source, option)} />
      </>}
    </div>
  </div>;
}
