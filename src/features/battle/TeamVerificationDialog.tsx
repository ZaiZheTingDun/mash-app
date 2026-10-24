import { useEffect, useRef, useState } from "react";
import { AlertDialog, Button, Flex, Text } from "@radix-ui/themes";
import { invoke, listen } from "../../tauri";
import type { TeamVerificationReport } from "../../types/teamVerification";

const KIND_LABELS = {
  servant: "从者",
  craftEssence: "概念礼装",
  mysticCode: "御主礼装",
};

export function TeamVerificationDialog() {
  const [report, setReport] = useState<TeamVerificationReport | null>(null);
  const [busy, setBusy] = useState(false);
  const submitting = useRef(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let revision = 0;
    const apply = (value: TeamVerificationReport | null) => {
      if (!active) return;
      setReport(value);
      setBusy(false);
      submitting.current = false;
      setError(null);
    };
    const unlisten = listen<TeamVerificationReport | null>("team-verification-updated", ({ payload }) => {
      revision += 1;
      apply(payload);
    });
    // Subscribe before querying; stale initial results must not resurrect a
    // dismissed report or overwrite a newer check.
    void unlisten.then(async () => {
      if (!active) return;
      const queriedRevision = revision;
      try {
        const value = await invoke<TeamVerificationReport | null>("get_team_verification");
        if (revision === queriedRevision) apply(value);
      } catch (err) {
        if (active) console.warn("获取编队检查状态失败", err);
      }
    });
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  const submit = async (decision: "retry" | "skip" | "stop") => {
    if (report == null || submitting.current) return;
    submitting.current = true;
    setBusy(true);
    setError(null);
    try {
      if (decision === "stop") {
        await invoke("stop_automation");
      } else {
        await invoke("resolve_team_verification", {
          runId: report.runId,
          checkId: report.checkId,
          decision,
        });
      }
      // The backend dismisses the report when it consumes the decision.
    } catch (err) {
      submitting.current = false;
      setBusy(false);
      setError(String(err));
    }
  };

  return (
    <AlertDialog.Root open={report != null}>
      <AlertDialog.Content maxWidth="560px" onEscapeKeyDown={(event) => event.preventDefault()}>
        <AlertDialog.Title>编队检查未通过</AlertDialog.Title>
        <AlertDialog.Description size="2">
          自动化已暂停，请检查游戏中的编队。跳过后，本次运行将不再检查从者、概念礼装和御主礼装。
        </AlertDialog.Description>
        <ul>
          {report?.issues.map((issue, index) => (
            <li key={index}>
              <Text size="2">
                {issue.slotIndex != null ? `第 ${issue.slotIndex + 1} 位` : ""}
                {KIND_LABELS[issue.kind]}：{issue.expectedName} — {issue.reason}
              </Text>
            </li>
          ))}
        </ul>
        {error != null && <Text as="p" size="2" color="red" role="alert">{error}</Text>}
        <Flex justify="end" gap="3" mt="4" wrap="wrap">
          <Button type="button" color="gray" variant="soft" disabled={busy} onClick={() => void submit("stop")}>停止任务</Button>
          <Button type="button" variant="soft" disabled={busy} onClick={() => void submit("retry")}>重新检查</Button>
          <Button type="button" disabled={busy} onClick={() => void submit("skip")}>跳过检测并继续</Button>
        </Flex>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}
