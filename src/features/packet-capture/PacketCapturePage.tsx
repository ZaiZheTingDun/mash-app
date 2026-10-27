import { useEffect, useRef, useState } from "react";
import { Box, Button, Flex, Text, TextArea } from "@radix-ui/themes";
import { PageHeader } from "../../components/common/PageHeader";
import type { CaptureConfiguration, CaptureStarted, CertificateInstallation, PacketCaptureResponseState } from "../../types/packetCapture";
import { configurePacketCapture, getPacketCaptureResponse, installCaptureCertificate, startPacketCapture } from "./packetCaptureApi";

const actions = {
  certificate: installCaptureCertificate,
  configuration: configurePacketCapture,
  capture: startPacketCapture,
};
const pendingMessages = {
  certificate: "正在安装证书，请留意设备上的授权提示…",
  configuration: "正在配置 Reqable，完成后抓包保持关闭…",
  capture: "正在启动抓包，请留意设备上的 VPN 授权提示…",
};

export function PacketCapturePage({ onBack }: { onBack: () => void }) {
  const [operation, setOperation] = useState<keyof typeof actions | null>(null);
  const [result, setResult] = useState<CertificateInstallation | CaptureConfiguration | CaptureStarted | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [responseState, setResponseState] = useState<PacketCaptureResponseState | null>(null);
  const [responseError, setResponseError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const busy = operation !== null;

  useEffect(() => {
    if (operation !== null) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = async () => {
      try {
        const state = await getPacketCaptureResponse();
        if (cancelled) return;
        setResponseState(state);
        setResponseError(null);
        if (state.phase === "waiting") timer = setTimeout(() => void refresh(), 1000);
      } catch (error) {
        if (!cancelled) setResponseError(String(error));
      }
    };
    void refresh();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [operation]);

  const perform = async (action: keyof typeof actions) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setOperation(action);
    setResult(null);
    setError(null);
    setResponseState(null);
    setResponseError(null);
    try {
      setResult(await actions[action]());
    } catch (error) {
      setError(String(error));
    } finally {
      inFlight.current = false;
      setOperation(null);
    }
  };

  return (
    <main className="main-content-inner workspace-page" aria-label="抓包">
      <PageHeader title="抓包" english="PACKET CAPTURE" onBack={onBack} backDisabled={busy}>
        {null}
      </PageHeader>
      <Box p="4">
        <Flex direction="column" align="start" gap="3">
          <Text size="3" weight="medium">Reqable</Text>
          <Text size="2" color="gray">设备需已安装 Reqable 并具备 root 权限。本工具安装的证书在设备重启后需重新安装。</Text>
          <Flex gap="3" wrap="wrap">
            <Button loading={operation === "certificate"} disabled={busy} onClick={() => void perform("certificate")}>安装证书</Button>
            <Button loading={operation === "configuration"} disabled={busy} onClick={() => void perform("configuration")}>自动配置</Button>
            <Button loading={operation === "capture"} disabled={busy} onClick={() => void perform("capture")}>开始抓包</Button>
          </Flex>
          {operation && <Text size="2" color="gray" role="status">{pendingMessages[operation]}</Text>}
          {result && !("alreadyActive" in result && responseState?.response) && (
            <Flex direction="column" gap="1" role="status">
              <Text size="2" color="green">{result.message}</Text>
              <Text size="2" color="gray">设备：{result.serial}</Text>
            </Flex>
          )}
          {error && <Text size="2" color="red" role="alert">{error}</Text>}
          {!busy && responseError && <Text size="2" color="red" role="alert">{responseError}</Text>}
          {!busy && responseState?.phase === "waiting" && <Text size="2" color="gray" role="status">{responseState.message}</Text>}
          {!busy && responseState?.phase === "found" && <Text size="2" color="green" role="status">{responseState.message}</Text>}
          {!busy && responseState?.phase === "error" && <Text size="2" color="red" role="alert">{responseState.message}</Text>}
          {!busy && responseState?.response && (
            <Flex direction="column" gap="2" width="100%">
              <Text size="3" weight="medium">toplogin 响应</Text>
              <Text size="2" color="gray" wrap="wrap">{responseState.response.url}</Text>
              <Text size="2" color="gray">HTTP {responseState.response.statusCode} · {responseState.response.byteLength.toLocaleString()} 字节 · {responseState.response.format}</Text>
              <TextArea aria-label="toplogin 响应正文" value={responseState.response.body} readOnly rows={20} style={{ fontFamily: "monospace" }} />
            </Flex>
          )}
        </Flex>
      </Box>
    </main>
  );
}
