import { useEffect, useMemo, useState } from "react";
import { convertFileSrc, invoke } from "../../tauri";
import type { Servant } from "../../types/servant";

export function useServantFaceImages(servants: (Servant | null)[]) {
  const [faces, setFaces] = useState<Record<string, string | null>>({});
  const requests = useMemo(
    () =>
      servants
        .filter((servant): servant is Servant => Boolean(servant))
        .map((servant) => ({
          variantKey: servant.variantKey,
          servantId: servant.id,
          faceId: servant.faceId ?? null,
        }))
        .sort((a, b) => a.variantKey.localeCompare(b.variantKey)),
    [servants]
  );
  const requestKey = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(requestKey) as typeof requests;
    const missing = parsed.filter(({ variantKey }) => !(variantKey in faces));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_face_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then(
            (path) =>
              [request.variantKey, path ? convertFileSrc(path) : null] as const
          )
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setFaces((prev) => {
        const next = { ...prev };
        for (const [variantKey, src] of results) next[variantKey] = src;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // Keep this effect keyed by the servant request set; including `faces`
    // would re-run every time the cache is filled.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestKey]);

  return faces;
}
