import { cloneElement, useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactElement } from "react";
import { Avatar, Flex, IconButton, Popover } from "@radix-ui/themes";
import { MagicWandIcon } from "@radix-ui/react-icons";
import type { Servant } from "../../types/servant";
import type { PreparationAction } from "../../types/command";
import { CommandSkillSummary } from "../../components/common/CommandSkillSummary";
import type { PartySlot, PrepSource } from "./battleSceneModel";
import { makePreparationAction, startPreparationSkill, type PreparationContext, type PreparationResult } from "./preparationActions";

// Temporary exception for Mash's third battle variant (800190 / 800200 / 800210).
// Keep the two runtime forms and their target rules here so this can be removed together.
const FORMS = [
  { id: "whiteWall", skillFormId: 2550, name: "时为朦胧的白垩之壁 A", icon: "skill_00419.png" },
  { id: "purpleBullet", skillFormId: 2477450, name: "测定时间的紫弹之薪 B", icon: "skill_00302.png" },
] as const;
type MashSkillTwoForm = typeof FORMS[number]["id"];

function prepareMashSkillTwo(context: PreparationContext, source: PrepSource, form: MashSkillTwoForm, target: PartySlot | null): PreparationResult {
  const skillFormId = FORMS.find(entry => entry.id === form)!.skillFormId;
  const result: PreparationResult = form === "purpleBullet" ? {
    type: "action",
    action: makePreparationAction({ step: "target", source, option: "skill_2", skillFormId }, null, context.members),
  } : startPreparationSkill({ ...context, skillTargetStatus: () => "needsTarget" }, source, "skill_2", target);
  // The shared flow still honors "关闭自动技能目标识别" for the targeted form.
  if (result.type === "action" && result.action.type === "servant") return { ...result, action: { ...result.action, skillFormId } };
  if (result.type === "draft" && result.draft.step === "target") return { ...result, draft: { ...result.draft, skillFormId } };
  return result;
}

export function MashSkillTwoSummary({ action, servant, src, name, fallback }: {
  action: PreparationAction; servant: Servant | null; src: string | null; name: string; fallback: string;
}) {
  const form = action.type === "servant" && action.skill === "skill_2" && servant?.id === 1 && servant.variantKey === "1:3"
    ? FORMS.find(entry => entry.skillFormId === action.skillFormId) : null;
  return <CommandSkillSummary src={form ? src?.replace(/skill_\d+\.png(?=$|[?#])/, form.icon) ?? null : src}
    name={form?.name ?? name} fallback={fallback} />;
}

interface Props {
  servant: Servant | null;
  skill: string;
  iconSrc?: string;
  context: PreparationContext;
  source: PrepSource;
  target: PartySlot | null;
  onResult: (result: PreparationResult) => void;
  children: ReactElement<ButtonHTMLAttributes<HTMLButtonElement>>;
}

export function MashSkillTwoChoice({ servant, skill, iconSrc, children, context, source, target, onResult }: Props) {
  if (servant?.id !== 1 || servant.variantKey !== "1:3" || skill !== "skill_2") return children;
  return <FormPicker key={String(children.props.disabled)} iconSrc={iconSrc} onChoose={form => onResult(prepareMashSkillTwo(context, source, form, target))}>{children}</FormPicker>;
}

function FormPicker({ children, iconSrc, onChoose }: Pick<Props, "children" | "iconSrc"> & { onChoose: (form: MashSkillTwoForm) => void }) {
  const [open, setOpen] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const keyboardOpen = useRef(false);
  const choosing = useRef(false);
  const cancelClose = () => {
    if (closeTimer.current != null) clearTimeout(closeTimer.current);
    closeTimer.current = null;
  };
  const closeSoon = () => {
    cancelClose();
    closeTimer.current = setTimeout(() => setOpen(false), 150);
  };
  useEffect(() => () => { if (closeTimer.current != null) clearTimeout(closeTimer.current); }, []);
  return <Popover.Root open={open} onOpenChange={setOpen}>
    <Popover.Trigger onClick={event => {
      event.preventDefault();
      cancelClose(); keyboardOpen.current = event.detail === 0; setOpen(true);
    }} onPointerEnter={() => {
      if (children.props.disabled) return;
      cancelClose(); keyboardOpen.current = false; setOpen(true);
    }} onPointerLeave={closeSoon} onKeyDown={event => {
      if (event.key === "ArrowDown") { event.preventDefault(); keyboardOpen.current = true; setOpen(true); }
    }}>
      {cloneElement(children, { onClick: undefined })}
    </Popover.Trigger>
    <Popover.Content size="1" width="max-content" side="bottom" align="center" sideOffset={6}
      aria-label="选择玛修二技能形态" onPointerEnter={cancelClose} onPointerLeave={closeSoon}
      onOpenAutoFocus={event => { if (!keyboardOpen.current) event.preventDefault(); }}
      onCloseAutoFocus={event => { if (choosing.current) { event.preventDefault(); choosing.current = false; } }}>
      <Flex direction="row" gap="2">
        {FORMS.map(form => <IconButton key={form.id} variant="soft" color="gray" disabled={children.props.disabled}
          aria-label={form.name} className={children.props.className}
          style={{ width: 46, height: 46, flexShrink: 0 }}
          onClick={() => { cancelClose(); choosing.current = true; setOpen(false); onChoose(form.id); }}>
          <Avatar src={iconSrc?.replace(/skill_\d+\.png(?=$|[?#])/, form.icon)}
            alt="" fallback={<MagicWandIcon />} radius="small" size="2" />
        </IconButton>)}
      </Flex>
    </Popover.Content>
  </Popover.Root>;
}
