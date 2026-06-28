import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { Avatar, Box, Flex, Text, Popover, Button, Spinner, Checkbox, Select, IconButton } from "@radix-ui/themes";
import {
  HamburgerMenuIcon,
  Link2Icon,
  DesktopIcon,
  MagnifyingGlassIcon,
  MoonIcon,
  PersonIcon,
  SunIcon,
  Cross1Icon,
  GearIcon,
} from "@radix-ui/react-icons";
import { convertFileSrc, invoke, listen } from "../../tauri";
import type {
  ActionLogMeta,
  OperationLogEntry,
  AttackLogMeta,
} from "../../operationLog";
import {
  useServantSkillIcons,
  type SkillIcons,
} from "../team/useServantSkillIcons";
import { SERVER_LABELS, type Server } from "../../types/server";
import type { Servant } from "../../types/servant";
import type { AppTheme, AppThemePreference } from "../../types/theme";

type LogLevel = "info" | "warn" | "debug" | "localDebug";

interface AdbStatus {
  connected: boolean;
  deviceName: string | null;
}

interface AdbResetStep {
  command: string;
  success: boolean;
  status: number | null;
  stdout: string;
  stderr: string;
}

interface AdbResetResult {
  ok: boolean;
  steps: AdbResetStep[];
}

interface AdbResetStatusEvent {
  message: string;
  step?: AdbResetStep | null;
  done: boolean;
  ok?: boolean | null;
}

interface AutomationStatusEvent {
  state: string;
}

const POLL_INTERVAL_MS = 3000;
const COMMAND_CARD_LABELS = ["指令卡一", "指令卡二", "指令卡三", "指令卡四", "指令卡五"];

function useOperationLogFaces(
  operationLogs: OperationLogEntry[],
  servantById: Map<number, Servant>
): Record<number, string | null> {
  const [faces, setFaces] = useState<Record<number, string | null>>({});
  const requests = useMemo(() => {
    const ids = new Set<number>();
    for (const entry of operationLogs) {
      const attack = entry.attack;
      if (attack) {
        attack.frontServantIds.forEach((id) => {
          if (id != null) ids.add(id);
        });
        attack.candidateServantIds?.forEach((id) => ids.add(id));
        attack.commandCards?.forEach((card) => {
          if (card.servantId != null) ids.add(card.servantId);
        });
        if (attack.selectedPick?.servantId != null) ids.add(attack.selectedPick.servantId);
      }
      const action = entry.action;
      if (!action) continue;
      if (action.kind === "servantSkill" && action.servantId != null) {
        ids.add(action.servantId);
      }
      if (
        (action.kind === "servantSkill"
          || action.kind === "equipmentSkill"
          || action.kind === "commandSpell")
        && action.targetServantId != null
      ) {
        ids.add(action.targetServantId);
      }
      if (action.kind === "orderChange") {
        if (action.frontServantId != null) ids.add(action.frontServantId);
        if (action.backServantId != null) ids.add(action.backServantId);
      }
      if (action.kind === "skippedAction" && action.servantId != null) {
        ids.add(action.servantId);
      }
    }
    return Array.from(ids)
      .map((servantId) => {
        const servant = servantById.get(servantId);
        return {
          servantId,
          faceId: servant?.faceId ?? null,
        };
      })
      .sort((a, b) => a.servantId - b.servantId);
  }, [operationLogs, servantById]);
  const key = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(key) as typeof requests;
    const missing = parsed.filter(({ servantId }) => !(servantId in faces));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_face_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then((path) => [request.servantId, path ? convertFileSrc(path) : null] as const)
          .catch(() => [request.servantId, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setFaces((prev) => {
        const next = { ...prev };
        for (const [servantId, src] of results) next[servantId] = src;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // Keep this effect keyed by the request set; including `faces`
    // would refetch after every cache fill.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return faces;
}

function servantName(servantById: Map<number, Servant>, servantId: number | null | undefined): string {
  if (servantId == null) return "未识别从者";
  const servant = servantById.get(servantId);
  return servant?.name_cn_server || servant?.name_cn || `从者 ${servantId}`;
}

function OperationLogFace({
  servantId,
  servantById,
  faceSrcByServantId,
  muted = false,
  highlighted = false,
}: {
  servantId: number | null | undefined;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
  muted?: boolean;
  highlighted?: boolean;
}) {
  const label = servantName(servantById, servantId);
  const src = servantId != null ? faceSrcByServantId[servantId] : null;
  return (
    <span
      className={[
        "operation-log-face",
        muted ? "operation-log-face--muted" : "",
        highlighted ? "operation-log-face--highlighted" : "",
      ].filter(Boolean).join(" ")}
      aria-label={label}
      title={label}
    >
      <Avatar
        src={src ?? undefined}
        alt=""
        fallback={<PersonIcon width={13} height={13} aria-hidden />}
        radius="none"
        size="1"
        draggable={false}
      />
    </span>
  );
}

function SuitDot({ suit }: { suit: string | null | undefined }) {
  const normalized = suit === "a" || suit === "b" || suit === "q" ? suit : "unknown";
  return (
    <span
      className={`operation-log-suit-dot operation-log-suit-dot--${normalized}`}
      aria-label={suitLabel(suit)}
      title={suitLabel(suit)}
    />
  );
}

function suitLabel(suit: string | null | undefined): string {
  if (suit === "q") return "Quick";
  if (suit === "a") return "Arts";
  if (suit === "b") return "Buster";
  return "未知色卡";
}

function OperationLogMessage({
  entry,
  servantById,
  faceSrcByServantId,
  skillIcons,
}: {
  entry: OperationLogEntry;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
  skillIcons: Record<string, SkillIcons>;
}) {
  if (entry.action) {
    return (
      <ActionLogMessage
        action={entry.action}
        message={entry.message}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
        skillIcons={skillIcons}
      />
    );
  }
  const attack = entry.attack;
  if (!attack) return <>{entry.message}</>;
  if (attack.selectedPick) {
    return (
      <SelectedPickLogMessage
        attack={attack}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
    );
  }
  if (attack.readyNpSlots) {
    return (
      <ReadyNpLogMessage
        attack={attack}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
    );
  }
  if (attack.commandCards) {
    return (
      <CommandCardsLogMessage
        attack={attack}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
    );
  }
  if (attack.candidateServantIds) {
    return (
      <span className="operation-log-rich">
        <span>指令卡候选从者:</span>
        <span className="operation-log-face-row">
          {attack.candidateServantIds.map((servantId, index) => (
            <OperationLogFace
              key={`${servantId}-${index}`}
              servantId={servantId}
              servantById={servantById}
              faceSrcByServantId={faceSrcByServantId}
            />
          ))}
        </span>
      </span>
    );
  }
  return <>{entry.message}</>;
}

function ActionLogMessage({
  action,
  message,
  servantById,
  faceSrcByServantId,
  skillIcons,
}: {
  action: ActionLogMeta;
  message: string;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
  skillIcons: Record<string, SkillIcons>;
}) {
  if (action.kind === "servantSkill") {
    return (
      <span className="operation-log-rich">
        <span>从者技能:</span>
        <OperationLogFace
          servantId={action.servantId}
          servantById={servantById}
          faceSrcByServantId={faceSrcByServantId}
        />
        <OperationLogSkillIcon
          servantId={action.servantId}
          skillIndex={action.skillIndex}
          servantById={servantById}
          skillIcons={skillIcons}
        />
        <ActionTarget
          servantId={action.targetServantId}
          servantById={servantById}
          faceSrcByServantId={faceSrcByServantId}
        />
      </span>
    );
  }
  if (action.kind === "equipmentSkill") {
    return (
      <span className="operation-log-rich">
        <span>御主技能:</span>
        <span
          className="operation-log-skill-icon operation-log-skill-icon--fallback"
          aria-label={`御主技能 ${action.skillIndex + 1}`}
          title={`御主技能 ${action.skillIndex + 1}`}
        >
          御{action.skillIndex + 1}
        </span>
        <ActionTarget
          servantId={action.targetServantId}
          servantById={servantById}
          faceSrcByServantId={faceSrcByServantId}
        />
      </span>
    );
  }
  if (action.kind === "commandSpell") {
    return (
      <span className="operation-log-rich">
        <span>令咒:</span>
        <span className="operation-log-skill-icon operation-log-skill-icon--command">
          令
        </span>
        <span>{action.spell === "np_release" ? "宝具解放" : "灵基复原"}</span>
        <ActionTarget
          servantId={action.targetServantId}
          servantById={servantById}
          faceSrcByServantId={faceSrcByServantId}
        />
      </span>
    );
  }
  if (action.kind === "orderChange") {
    return (
      <span className="operation-log-rich">
        <span>Order Change:</span>
        <OperationLogFace
          servantId={action.frontServantId}
          servantById={servantById}
          faceSrcByServantId={faceSrcByServantId}
        />
        <span>↔</span>
        <OperationLogFace
          servantId={action.backServantId}
          servantById={servantById}
          faceSrcByServantId={faceSrcByServantId}
        />
      </span>
    );
  }
  return (
    <span className="operation-log-rich">
      <span>{message}</span>
      <OperationLogFace
        servantId={action.servantId}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
    </span>
  );
}

function OperationLogSkillIcon({
  servantId,
  skillIndex,
  servantById,
  skillIcons,
}: {
  servantId: number | null;
  skillIndex: number;
  servantById: Map<number, Servant>;
  skillIcons: Record<string, SkillIcons>;
}) {
  const servant = servantId == null ? null : servantById.get(servantId) ?? null;
  const entry = servant ? skillIcons[servant.variantKey]?.[skillIndex] : null;
  const label = entry?.name || `技能 ${skillIndex + 1}`;
  return (
    <span className="operation-log-skill-icon" aria-label={label} title={label}>
      <Avatar
        src={entry?.src ?? undefined}
        alt=""
        fallback={String(skillIndex + 1)}
        radius="small"
        size="1"
        draggable={false}
      />
    </span>
  );
}

function ActionTarget({
  servantId,
  servantById,
  faceSrcByServantId,
}: {
  servantId: number | null;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
}) {
  if (servantId == null) return null;
  return (
    <>
      <span>→</span>
      <OperationLogFace
        servantId={servantId}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
    </>
  );
}

function CommandCardsLogMessage({
  attack,
  servantById,
  faceSrcByServantId,
}: {
  attack: AttackLogMeta;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
}) {
  return (
    <span className="operation-log-rich">
      <span>指令卡:</span>
      <span className="operation-log-card-list">
        {attack.commandCards?.map((card, index) => (
          <span className="operation-log-card-item" key={`${card.slot}-${index}`}>
            <OperationLogFace
              servantId={card.servantId}
              servantById={servantById}
              faceSrcByServantId={faceSrcByServantId}
            />
            <SuitDot suit={card.suit} />
          </span>
        ))}
      </span>
    </span>
  );
}

function ReadyNpLogMessage({
  attack,
  servantById,
  faceSrcByServantId,
}: {
  attack: AttackLogMeta;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
}) {
  const ready = new Set(attack.readyNpSlots ?? []);
  return (
    <span className="operation-log-rich">
      <span>宝具就绪:</span>
      <span className="operation-log-face-row">
        {attack.frontServantIds.map((servantId, index) => (
          <OperationLogFace
            key={index}
            servantId={servantId}
            servantById={servantById}
            faceSrcByServantId={faceSrcByServantId}
            muted={!ready.has(index)}
            highlighted={ready.has(index)}
          />
        ))}
      </span>
    </span>
  );
}

function SelectedPickLogMessage({
  attack,
  servantById,
  faceSrcByServantId,
}: {
  attack: AttackLogMeta;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
}) {
  const pick = attack.selectedPick;
  if (!pick) return null;
  const servantId = pick.servantId ?? attack.frontServantIds[pick.slot] ?? null;
  return (
    <span className="operation-log-rich">
      <span>{pick.step}/{pick.total} 选择</span>
      <PrioritySource
        value={pick.fromPriority}
        attack={attack}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
      <span>→</span>
      {pick.kind === "np" ? (
        <span className="operation-log-pick-result">
          <span>宝具</span>
          <OperationLogFace
            servantId={servantId}
            servantById={servantById}
            faceSrcByServantId={faceSrcByServantId}
          />
        </span>
      ) : (
        <span className="operation-log-pick-result">
          <span>{COMMAND_CARD_LABELS[pick.slot] ?? `指令卡${pick.slot + 1}`}</span>
          <OperationLogFace
            servantId={servantId}
            servantById={servantById}
            faceSrcByServantId={faceSrcByServantId}
          />
          <SuitDot suit={pick.suit} />
        </span>
      )}
    </span>
  );
}

function PrioritySource({
  value,
  attack,
  servantById,
  faceSrcByServantId,
}: {
  value: string | null;
  attack: AttackLogMeta;
  servantById: Map<number, Servant>;
  faceSrcByServantId: Record<number, string | null>;
}) {
  const match = value?.match(/^servant_(\d+)_(np|buster|arts|quick|all)$/);
  if (!match) return <span>{value ?? "补位"}</span>;
  const slot = Number(match[1]) - 1;
  const kind = match[2];
  const suit = kind === "buster" ? "b" : kind === "arts" ? "a" : kind === "quick" ? "q" : null;
  return (
    <span className="operation-log-pick-result">
      <OperationLogFace
        servantId={attack.frontServantIds[slot] ?? null}
        servantById={servantById}
        faceSrcByServantId={faceSrcByServantId}
      />
      {kind === "np" ? <span>宝具</span> : suit ? <SuitDot suit={suit} /> : <span>任意卡</span>}
    </span>
  );
}

interface StatusBarProps {
  onOpenSettings?: () => void;
  /** Wired by `App.tsx` to switch the main view to the CV debug page.
   * Optional so existing tests (and any callers that don't need the
   * shortcut) can still mount `<StatusBar />` with no props. */
  onOpenDebug?: () => void;
  theme?: AppTheme;
  themePreference?: AppThemePreference;
  onThemeChange?: (theme: AppThemePreference) => void;
  operationLogs?: OperationLogEntry[];
  servants?: Servant[];
  operationLogOpen?: boolean;
  onOperationLogOpenChange?: (open: boolean) => void;
  updateAvailable?: boolean;
  updateChecking?: boolean;
  updateInstalling?: boolean;
  updateProgressText?: string | null;
  onInstallUpdate?: () => void;
  onLogEntry?: (message: string, level?: LogLevel) => void;
}

export function StatusBar({
  onOpenSettings,
  onOpenDebug,
  theme,
  themePreference,
  onThemeChange,
  operationLogs = [],
  servants = [],
  operationLogOpen = false,
  onOperationLogOpenChange,
  updateAvailable = false,
  updateChecking = false,
  updateInstalling = false,
  updateProgressText,
  onInstallUpdate,
  onLogEntry,
}: StatusBarProps = {}) {
  const [status, setStatus] = useState<AdbStatus>({ connected: false, deviceName: null });
  const [checking, setChecking] = useState(false);
  const [resettingAdb, setResettingAdb] = useState(false);
  const [useBluestack, setUseBluestack] = useState(false);
  const [server, setServer] = useState<Server>("JP");
  // The server selector must be locked while the runner is mid-run: the
  // sidecar already pinned templates / OCR for the previous server when
  // it spawned, so flipping the global setting now would silently
  // desync. Both battle automation and servant-enhancement automation
  // pin server-specific OCR/templates, so either one should lock the
  // selector until it exits.
  const [battleRunnerRunning, setBattleRunnerRunning] = useState(false);
  const [enhancementRunnerRunning, setEnhancementRunnerRunning] = useState(false);
  // Debug entries (CV anchor positions, swipe distances, …) are
  // hidden by default to keep the operation log readable during a
  // normal run; flip this toggle to surface them for triage.
  const [showDebugLogs, setShowDebugLogs] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);
  const runnerRunning = battleRunnerRunning || enhancementRunnerRunning;
  const servantById = useMemo(() => {
    const byId = new Map<number, Servant>();
    for (const servant of servants) {
      if (!byId.has(servant.id)) byId.set(servant.id, servant);
    }
    return byId;
  }, [servants]);
  const operationLogFaces = useOperationLogFaces(operationLogs, servantById);
  const operationLogSkillServants = useMemo(() => {
    const ids = new Set<number>();
    for (const entry of operationLogs) {
      if (entry.action?.kind === "servantSkill" && entry.action.servantId != null) {
        ids.add(entry.action.servantId);
      }
    }
    return Array.from(ids)
      .map((id) => servantById.get(id) ?? null)
      .filter((servant): servant is Servant => servant != null);
  }, [operationLogs, servantById]);
  const operationLogSkillIcons = useServantSkillIcons(operationLogSkillServants);

  const visibleOperationLogs = useMemo(
    () => {
      const localDebugVisible = import.meta.env.DEV;
      return operationLogs.filter((entry) => {
        if (entry.level === "localDebug") return showDebugLogs && localDebugVisible;
        if (entry.level === "debug") return showDebugLogs;
        return true;
      });
    },
    [operationLogs, showDebugLogs],
  );
  // Show the count of user-facing entries even when debug is on; keep
  // diagnostics excluded, but include warnings because they are actionable.
  const userFacingLogCount = useMemo(
    () => operationLogs.filter((entry) => entry.level !== "debug" && entry.level !== "localDebug").length,
    [operationLogs],
  );

  useEffect(() => {
    invoke<boolean>("get_use_bluestack").then(setUseBluestack).catch(() => {});
    invoke<Server>("get_server").then(setServer).catch(() => {});
  }, []);

  const pollAdb = useCallback(() => {
    invoke<AdbStatus>("check_adb")
      .then(setStatus)
      .catch(() => setStatus({ connected: false, deviceName: null }));
  }, []);

  useEffect(() => {
    pollAdb();
    const id = setInterval(pollAdb, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [pollAdb]);

  useEffect(() => {
    const unlistenBattle = listen<AutomationStatusEvent>(
      "automation-status",
      (event) => {
        const state = event.payload.state ?? "";
        setBattleRunnerRunning(state.includes("Starting") || state.includes("Running"));
      }
    );
    const unlistenEnhancement = listen<AutomationStatusEvent>(
      "enhancement-automation-status",
      (event) => {
        const state = event.payload.state ?? "";
        setEnhancementRunnerRunning(state.includes("Starting") || state.includes("Running"));
      }
    );
    return () => {
      unlistenBattle.then((fn) => fn());
      unlistenEnhancement.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<AdbResetStatusEvent>("adb-reset-status", (event) => {
      const { done, message } = event.payload;
      onLogEntry?.(message);
      if (done) {
        setResettingAdb(false);
        pollAdb();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [onLogEntry, pollAdb]);

  const handleConnect = useCallback(() => {
    setChecking(true);
    invoke<AdbStatus>("check_adb")
      .then(setStatus)
      .catch(() => setStatus({ connected: false, deviceName: null }))
      .finally(() => setChecking(false));
  }, []);

  const handleResetAdb = useCallback(() => {
    setResettingAdb(true);
    onOperationLogOpenChange?.(true);
    invoke<AdbResetResult>("reset_bluestacks_adb_connection")
      .then((result) => {
        if (result.steps.length === 0) return;
        result.steps.forEach((step) => {
          const statusText = step.status == null ? "spawn failed" : `exit ${step.status}`;
          const output = [step.stdout, step.stderr].filter(Boolean).join(" | ");
          onLogEntry?.(
            `${step.success ? "成功" : "失败"}: ${step.command} (${statusText})${
              output ? ` - ${output}` : ""
            }`,
          );
        });
        onLogEntry?.(result.ok ? "ADB 链接重置完成" : "ADB 链接重置完成，但存在失败命令");
        setResettingAdb(false);
        pollAdb();
      })
      .catch((err) => {
        onLogEntry?.(`ADB 链接重置失败: ${String(err)}`);
        setStatus({ connected: false, deviceName: null });
        setResettingAdb(false);
      });
  }, [onLogEntry, onOperationLogOpenChange, pollAdb]);

  const handleBluestackToggle = useCallback((checked: boolean) => {
    setUseBluestack(checked);
    invoke("set_use_bluestack", { value: checked }).catch(() => {});
  }, []);

  const handleServerChange = useCallback((value: string) => {
    if (value !== "JP" && value !== "CN") return;
    const next = value as Server;
    const previous = server;
    // Optimistic update; revert + log on backend rejection (e.g. the
    // runner started between this render and the IPC round-trip).
    setServer(next);
    invoke("set_server", { value: next }).catch((err) => {
      console.error("set_server failed", err);
      setServer(previous);
    });
  }, [server]);

  const handleThemeToggle = useCallback(() => {
    if (!theme || !onThemeChange) return;
    const currentPreference = themePreference ?? theme;
    const nextPreference =
      currentPreference === "light"
        ? "dark"
        : currentPreference === "dark"
          ? "system"
          : "light";
    onThemeChange(nextPreference);
  }, [theme, themePreference, onThemeChange]);

  useEffect(() => {
    if (!operationLogOpen) return;
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [visibleOperationLogs, operationLogOpen]);

  return (
    <>
      {operationLogOpen && (
        <Box className="operation-log-panel">
          <Flex align="center" justify="between" className="operation-log-header">
            <Text size="1" weight="bold">操作日志</Text>
            <Flex align="center" gap="2">
              <label className="operation-log-debug-toggle">
                <Checkbox
                  size="1"
                  checked={showDebugLogs}
                  onCheckedChange={(checked) => setShowDebugLogs(checked === true)}
                />
                <Text size="1">显示调试</Text>
              </label>
              <IconButton
                variant="ghost"
                color="gray"
                aria-label="关闭操作日志"
                onClick={() => onOperationLogOpenChange?.(false)}
              >
                <Cross1Icon width={15} height={15} />
              </IconButton>
            </Flex>
          </Flex>
          <Box className="operation-log-list">
            {visibleOperationLogs.length === 0 && (
              <Text size="2" className="operation-log-placeholder">
                {operationLogs.length === 0 ? "等待启动…" : "没有可见日志（开启显示调试查看更多）"}
              </Text>
            )}
            {visibleOperationLogs.map((entry, index) => (
              <div
                key={index}
                className={[
                  "operation-log-entry",
                  entry.level === "warn" ? "operation-log-entry--warn" : "",
                  entry.level === "debug" || entry.level === "localDebug"
                    ? "operation-log-entry--debug"
                    : "",
                ].filter(Boolean).join(" ")}
              >
                <span className="operation-log-time">{entry.time}</span>
                <span className="operation-log-msg">
                  <OperationLogMessage
                    entry={entry}
                    servantById={servantById}
                    faceSrcByServantId={operationLogFaces}
                    skillIcons={operationLogSkillIcons}
                  />
                </span>
              </div>
            ))}
            <div ref={logEndRef} />
          </Box>
        </Box>
      )}
      <Flex className="status-bar" align="center" justify="between">
        <Flex align="center">
          {onOpenSettings && (
            <Button
              type="button"
              size="1"
              variant="solid"
              color="gray"
              className="status-settings-btn"
              onClick={onOpenSettings}
            >
              <GearIcon width={14} height={14} />
              <Text size="1">设置</Text>
            </Button>
          )}
          <Button
            type="button"
            size="1"
            variant="solid"
            color="gray"
            className="status-log-btn"
            aria-pressed={operationLogOpen}
            onClick={() => onOperationLogOpenChange?.(!operationLogOpen)}
          >
              <HamburgerMenuIcon width={14} height={14} />
              <Text size="1">
                操作日志{userFacingLogCount > 0 ? ` (${userFacingLogCount})` : ""}
              </Text>
          </Button>
        </Flex>

        <Flex align="center" justify="end" gap="1">
          {updateAvailable && (
            <Button
              type="button"
              size="1"
              variant="solid"
              className="status-update-btn"
              disabled={updateInstalling || updateChecking}
              onClick={onInstallUpdate}
            >
              {updateInstalling || updateChecking ? <Spinner size="1" /> : null}
              <Text size="1">
                {updateInstalling
                  ? updateProgressText ?? "更新中…"
                  : updateChecking
                    ? "检测中…"
                  : "更新"}
              </Text>
            </Button>
          )}
          {onOpenDebug && (
            <Button
              type="button"
              size="1"
              variant="solid"
              color="gray"
              className="status-debug-btn"
              onClick={onOpenDebug}
            >
              <MagnifyingGlassIcon width={12} height={12} />
              <Text size="1">CV 调试</Text>
            </Button>
          )}
          {theme && onThemeChange && (
            <Button
              type="button"
              size="1"
              variant="solid"
              color="gray"
              className="status-theme-btn"
              aria-label={`切换主题模式，当前${
                themePreference === "system"
                  ? "跟随系统"
                  : theme === "dark"
                    ? "深色"
                    : "浅色"
              }`}
              onClick={handleThemeToggle}
            >
              {themePreference === "system" ? (
                <DesktopIcon width={12} height={12} />
              ) : theme === "dark" ? (
                <MoonIcon width={12} height={12} />
              ) : (
                <SunIcon width={12} height={12} />
              )}
              <Text size="1">
                {themePreference === "system"
                  ? "系统"
                  : theme === "dark"
                    ? "深色"
                  : "浅色"}
              </Text>
            </Button>
          )}
          <Popover.Root>
            <Popover.Trigger>
              <Button type="button" size="1" variant="solid" color="gray" className="status-trigger">
                <span
                  className={`status-dot ${status.connected ? "connected" : "disconnected"}`}
                />
                <Text size="1" className="status-label">
                  {status.connected ? "游戏已连接" : "游戏未连接"}
                </Text>
              </Button>
            </Popover.Trigger>
            <Popover.Content side="top" align="end" size="1" className="status-popover">
              <Flex direction="column" gap="3">
                <Flex align="center" justify="between" gap="2">
                  <Text size="2">服务器</Text>
                  <Select.Root
                    size="1"
                    value={server}
                    onValueChange={handleServerChange}
                    disabled={runnerRunning}
                  >
                    <Select.Trigger aria-label="服务器" />
                    <Select.Content>
                      <Select.Item value="JP">{SERVER_LABELS.JP}</Select.Item>
                      <Select.Item value="CN">{SERVER_LABELS.CN}</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </Flex>

                <label className="bluestack-checkbox">
                  <Checkbox
                    size="1"
                    checked={useBluestack}
                    onCheckedChange={(checked) => handleBluestackToggle(checked === true)}
                  />
                  <Text size="2">使用 BlueStacks 模拟器</Text>
                </label>

                <Button
                  size="1"
                  variant="soft"
                  color="gray"
                  disabled={resettingAdb}
                  onClick={handleResetAdb}
                >
                  {resettingAdb ? <Spinner size="1" /> : <Link2Icon width={14} height={14} />}
                  {resettingAdb ? "重置中…" : "重置 ADB 链接"}
                </Button>

                {status.connected ? (
                  <Flex align="center" gap="2">
                    <DesktopIcon width={14} height={14} style={{ color: "var(--gray-10)", flexShrink: 0 }} />
                    <Text size="2" weight="medium">{status.deviceName}</Text>
                  </Flex>
                ) : (
                  <Flex direction="column" gap="2">
                    <Text size="2" color="gray">未检测到游戏设备</Text>
                    <Button
                      size="1"
                      variant="soft"
                      disabled={checking}
                      onClick={handleConnect}
                    >
                      {checking ? (
                        <Spinner size="1" />
                      ) : (
                        <Link2Icon width={14} height={14} />
                      )}
                      {checking ? "检测中…" : "连接"}
                    </Button>
                  </Flex>
                )}
              </Flex>
            </Popover.Content>
          </Popover.Root>
        </Flex>
      </Flex>
    </>
  );
}
