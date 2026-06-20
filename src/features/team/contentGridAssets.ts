import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "../../tauri";
import type { Servant } from "../../types/servant";

/**
 * Generic asset-path resolver hook. Walks a Rust command that takes a
 * single integer id and returns either an absolute file path or
 * `null`, then wraps the path with `convertFileSrc` so the result is
 * ready to drop into `<img src>`.
 */
function useAssetPaths(
  command: string,
  argKey: string,
  ids: number[],
): Record<number, string | null | undefined> {
  const [cache, setCache] = useState<Record<number, string | null>>({});

  const key = ids
    .filter((id, i, arr) => arr.indexOf(id) === i)
    .sort((a, b) => a - b)
    .join(",");

  useEffect(() => {
    const parsedIds = key
      ? key.split(",").map((s) => Number(s)).filter((n) => Number.isFinite(n))
      : [];
    const missing = parsedIds.filter((id) => !(id in cache));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((id) =>
        invoke<string | null>(command, { [argKey]: id })
          .then((path) => [id, path ? convertFileSrc(path) : null] as const)
          .catch(() => [id, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setCache((prev) => {
        const next = { ...prev };
        for (const [id, src] of results) {
          next[id] = src;
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `cache` intentionally excluded. The effect re-fires only when the
    // requested id set changes, which is exactly when we need new files.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, command, argKey]);

  return cache;
}

/**
 * Resolve full-art portrait paths for servants that don't have a cached
 * entry yet. Variants share the same base servant id, so the cache is
 * keyed by `variantKey` and the backend receives the variant asset id.
 */
export function usePortraits(servants: Servant[]): Record<string, string | null | undefined> {
  const [cache, setCache] = useState<Record<string, string | null>>({});
  const byVariant = new Map<string, Servant>();
  for (const servant of servants) {
    byVariant.set(servant.variantKey, servant);
  }
  const requests = Array.from(byVariant.values())
    .map((servant) => ({
      variantKey: servant.variantKey,
      servantId: servant.id,
      faceId: servant.faceId ?? null,
    }))
    .sort((a, b) => a.variantKey.localeCompare(b.variantKey));
  const key = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(key) as typeof requests;
    const missing = parsed.filter(({ variantKey }) => !(variantKey in cache));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_portrait_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then(
            (path) =>
              [
                request.variantKey,
                path ? convertFileSrc(path) : null,
              ] as const
          )
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setCache((prev) => {
        const next = { ...prev };
        for (const [variantKey, src] of results) {
          next[variantKey] = src;
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `cache` intentionally excluded. The effect re-fires only when the
    // servant variant set changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return cache;
}

/**
 * Resolve craft-essence card art paths through the Rust asset resolver.
 */
export function useCeCards(ceIds: number[]): Record<number, string | null | undefined> {
  return useAssetPaths("get_craft_essence_card_path", "craftEssenceId", ceIds);
}
