import { useEffect, useRef, useState, type ReactNode } from "react";
import { AlertDialog, Button, Flex, IconButton } from "@radix-ui/themes";
import { ChevronLeftIcon, ChevronRightIcon, PlusIcon, TrashIcon } from "@radix-ui/react-icons";
import { SectionRailIcon } from "../../components/common/SectionRailIcon";
import { TurnHelpTooltip } from "../../components/common/TurnHelpTooltip";

export type CommandStep = "prep" | "enemy" | "control" | "attack";
const STEPS = [
  { id: "prep", number: "01", label: "准备阶段", english: "PREPARATION" },
  { id: "enemy", number: "02", label: "敌方目标", english: "TARGET" },
  { id: "control", number: "03", label: "控制行动", english: "CONTROL ACTIONS" },
  { id: "attack", number: "04", label: "攻击阶段", english: "ATTACK" },
] as const;

interface CommandWorkspaceProps {
  wave: number; waveCount: number; turn: number; turns: { id: string }[];
  step: CommandStep; advanced?: boolean; busy?: boolean; canUndo?: boolean;
  onStep: (step: CommandStep) => void; onWave: (wave: number) => void; onTurn: (turn: number) => void;
  onAddWave?: () => void; onDeleteWave?: () => void; onAddTurn: () => void; onDeleteTurn: () => void; onUndo?: () => void;
  configuredWave?: boolean; configuredTurn?: boolean; error?: string | null; children: ReactNode;
}

export function CommandWorkspace({ wave, waveCount, turn, turns, step, advanced = false, busy = false, canUndo = false, onStep, onWave, onTurn, onAddWave, onDeleteWave, onAddTurn, onDeleteTurn, onUndo, configuredWave = false, configuredTurn = false, error, children }: CommandWorkspaceProps) {
  const [deleting, setDeleting] = useState<"wave" | "turn" | null>(null);
  const track = useRef<HTMLDivElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const visibleSteps = STEPS.filter(item => advanced || item.id !== "control")
    .map((item, index) => ({ ...item, number: String(index + 1).padStart(2, "0") }));
  const phase = visibleSteps.find(item => item.id === step)!;
  useEffect(() => {
    const strip=track.current;
    const active=strip?.querySelector<HTMLElement>('[aria-pressed="true"]');
    if(strip && active) {
      const left=active.offsetLeft-strip.offsetLeft;
      if(left < strip.scrollLeft) strip.scrollLeft=left;
      else if(left+active.offsetWidth > strip.scrollLeft+strip.clientWidth) strip.scrollLeft=left+active.offsetWidth-strip.clientWidth;
    }
  }, [turn, turns.length]);
  useEffect(() => { if (scroll.current) scroll.current.scrollTop = 0; }, [wave, turn, step]);
  const remove = (kind: "wave" | "turn") => {
    if (kind === "wave" ? configuredWave : configuredTurn) setDeleting(kind);
    else (kind === "wave" ? onDeleteWave : onDeleteTurn)?.();
  };
  return <div className={`command-workspace${advanced ? " is-grand" : ""}`}>
    <aside className="command-sidebar" aria-label="指令导航">
      <div className="command-rail-label"><SectionRailIcon /><span>WAVE</span>{onDeleteWave && <IconButton variant="ghost" color="gray" aria-label="删除当前 Battle" disabled={busy || waveCount <= 1} onClick={() => remove("wave")}><TrashIcon /></IconButton>}</div>
      <div className="command-wave-controls" aria-label={`第 ${wave + 1}/${waveCount} 面`}><strong>{String(wave + 1).padStart(2, "0")}</strong><span>/ {String(waveCount).padStart(2, "0")}</span>
        <IconButton variant="ghost" color="gray" aria-label="上一场战斗" disabled={busy || wave === 0} onClick={() => onWave(wave - 1)}><ChevronLeftIcon /></IconButton>
        <IconButton variant="ghost" color="gray" aria-label="下一场战斗" disabled={busy || wave + 1 === waveCount} onClick={() => onWave(wave + 1)}><ChevronRightIcon /></IconButton>
        {onAddWave && <IconButton variant="ghost" aria-label="添加 Battle" disabled={busy} onClick={onAddWave}><PlusIcon /></IconButton>}
      </div>
      <div className="command-divider" />
      <div className="command-rail-label"><SectionRailIcon /><span>TURN ({turns.length})</span><TurnHelpTooltip /><IconButton variant="ghost" aria-label="添加 Turn" disabled={busy} onClick={onAddTurn}><PlusIcon /></IconButton><IconButton variant="ghost" color="gray" aria-label="删除当前 Turn" disabled={busy || turns.length <= 1} onClick={() => remove("turn")}><TrashIcon /></IconButton></div>
      <div className="command-turn-track" ref={track}>{turns.map((item, index) => <button type="button" key={item.id} aria-label={`Turn ${index + 1}`} aria-pressed={turn === index} disabled={busy} onClick={() => onTurn(index)}><i className="command-diamond" />{String(index + 1).padStart(2, "0")}</button>)}</div>
      <div className="command-rail-label"><SectionRailIcon /><span>STEPS</span></div>
      <nav className="command-step-list">{visibleSteps.map(item => <button type="button" key={item.id} aria-pressed={step === item.id} onClick={() => onStep(item.id)}><span className="command-step-node"><span>{item.number}</span></span><span className="command-step-copy"><b>{item.label}</b><small>{item.english}</small></span></button>)}</nav>
      {canUndo && <Button variant="ghost" color="gray" className="command-undo" disabled={busy} onClick={onUndo}>撤销上次修改</Button>}
    </aside>
    <section className="command-content"><div className="command-phase-header"><small>WAVE {String(wave + 1).padStart(2, "0")} / TURN {String(turn + 1).padStart(2, "0")}</small><div><strong>{phase.number}</strong><span><h2>{phase.label}</h2><small>{phase.english}</small></span></div></div>
      {error && <div role="alert" className="command-error">{error}</div>}
      <div className="command-scroll-region" ref={scroll}><fieldset className="command-edit-surface" disabled={busy}>{children}</fieldset></div>
    </section>
    <AlertDialog.Root open={deleting != null} onOpenChange={open => { if (!open) setDeleting(null); }}><AlertDialog.Content maxWidth="440px"><AlertDialog.Title>删除当前{deleting === "wave" ? "面" : "回合"}？</AlertDialog.Title><AlertDialog.Description>该场景已有配置。删除后后续回合将按新顺序显示，可通过撤销恢复。</AlertDialog.Description><Flex justify="end" gap="3" mt="4"><AlertDialog.Cancel><Button variant="soft" color="gray">取消</Button></AlertDialog.Cancel><AlertDialog.Action><Button color="red" onClick={() => { (deleting === "wave" ? onDeleteWave : onDeleteTurn)?.(); setDeleting(null); }}>确认删除</Button></AlertDialog.Action></Flex></AlertDialog.Content></AlertDialog.Root>
  </div>;
}
