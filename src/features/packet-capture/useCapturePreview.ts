import { useEffect, useState } from "react";
import type { CapturePreview, CapturePreviewQuery } from "../../types/packetCapture";
import { getPacketCapturePreview } from "./packetCaptureApi";

export function useCapturePreview(recordId: string | null, query: CapturePreviewQuery) {
  const [retry, setRetry] = useState(0);
  const key = JSON.stringify({ recordId, query, retry });
  const [result, setResult] = useState<{ key: string; data: CapturePreview | null; error: string | null } | null>(null);
  useEffect(() => {
    if (!recordId) return;
    let cancelled = false;
    // Draft search/filter state stays in React; Rust owns the filtered projection.
    const timer = setTimeout(() => {
      void getPacketCapturePreview(recordId, query).then(data => {
        if (data.recordId !== recordId || data.tab !== query.tab) throw new Error("账户预览与所选记录不一致");
        if (!cancelled) setResult({ key, data, error: null });
      }).catch(error => {
        if (!cancelled) setResult({ key, data: null, error: String(error) });
      });
    }, query.search ? 180 : 0);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [recordId, query, key]);
  const current = result?.key === key ? result : null;
  return {
    data: current?.data ?? null,
    classes: result?.data?.recordId === recordId ? result.data.classes : [],
    error: current?.error ?? null,
    loading: recordId !== null && current === null,
    retry: () => setRetry(value => value + 1),
  };
}
