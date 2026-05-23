import { Box, Flex, Text } from "@radix-ui/themes";
import type {
  AttackButtonResultDto,
  BattleSceneResultDto,
  CommandCardMatchDto,
  EnhancementServantMatchResultDto,
  FindSupportsResultDto,
  NoblePhantasmMatchDto,
  ProbeResult,
  RunnerCoordinatesDto,
} from "./DebugPage";

function supportPanelShortLabel(panel: "owned" | "append" | null | undefined) {
  if (panel === "owned") return "持";
  if (panel === "append") return "追";
  return "";
}

function supportPanelLevels(levels: (number | null)[] | undefined) {
  if (!levels || levels.length === 0) return "";
  return levels.map((level) => (level == null ? "-" : String(level))).join("/");
}

/**
 * All overlay state the canvas renders. The host component owns the
 * data; this component is purely presentational so it can be reused by
 * the in-page debug canvas and by the popout window (which is fed the
 * same shape via Tauri events from the main DebugPage).
 */
export interface DebugCanvasState {
  imageSrc: string | null;
  probes: ProbeResult[];
  commandCards: CommandCardMatchDto[];
  noblePhantasms: NoblePhantasmMatchDto[];
  battleScene: BattleSceneResultDto | null;
  attackButton: AttackButtonResultDto | null;
  enhancementServantResult: EnhancementServantMatchResultDto | null;
  supportResult: FindSupportsResultDto | null;
  coordinates: RunnerCoordinatesDto | null;
  showCoordOverlay: boolean;
  visibleCoordGroups: string[];
}

export interface DebugCanvasProps extends DebugCanvasState {
  onImgLoad?: (e: React.SyntheticEvent<HTMLImageElement>) => void;
  onImgError?: () => void;
  /** Tweak the wrapper class — used by the popout to fill the viewport. */
  className?: string;
  /** Optional placeholder text when no screenshot is loaded. */
  placeholder?: string;
}

/**
 * Renders the screenshot plus every overlay layer (probes, command
 * cards, NPs, battle scene, attack button, supports, coord overlay).
 * Extracted from DebugPage so the popout window can reuse it without
 * duplicating ~370 lines of overlay JSX.
 */
export function DebugCanvas({
  imageSrc,
  probes,
  commandCards,
  noblePhantasms,
  battleScene,
  attackButton,
  enhancementServantResult,
  supportResult,
  coordinates,
  showCoordOverlay,
  visibleCoordGroups,
  onImgLoad,
  onImgError,
  className,
  placeholder = "点击 截取画面 开始",
}: DebugCanvasProps) {
  const visibleSet = new Set(visibleCoordGroups);

  if (!imageSrc) {
    return (
      <Box className={`debug-canvas-wrapper${className ? ` ${className}` : ""}`}>
        <Flex
          align="center"
          justify="center"
          className="debug-canvas-placeholder"
        >
          <Text size="2" color="gray">
            {placeholder}
          </Text>
        </Flex>
      </Box>
    );
  }

  return (
    <Box className={`debug-canvas-wrapper${className ? ` ${className}` : ""}`}>
      <Box className="debug-canvas">
        <img
          src={imageSrc}
          alt="screenshot"
          className="debug-canvas-img"
          onLoad={onImgLoad}
          onError={onImgError}
        />
        {probes.map((p, idx) =>
          p.match.found && p.match.region ? (
            <Box
              key={`box-${idx}`}
              className="debug-overlay-box"
              style={{
                left: `${p.match.region.x * 100}%`,
                top: `${p.match.region.y * 100}%`,
                width: `${p.match.region.w * 100}%`,
                height: `${p.match.region.h * 100}%`,
              }}
            >
              <span className="debug-overlay-label">
                {p.label} · {p.match.score.toFixed(2)}
              </span>
            </Box>
          ) : null
        )}
        {commandCards.flatMap((c) => {
          const overlays = [
            <Box
              key={`card-face-${c.slot}`}
              className="debug-overlay-box debug-overlay-face"
              style={{
                left: `${c.faceRegion.x * 100}%`,
                top: `${c.faceRegion.y * 100}%`,
                width: `${c.faceRegion.w * 100}%`,
                height: `${c.faceRegion.h * 100}%`,
              }}
            />,
            <Box
              key={`card-slot-${c.slot}`}
              className="debug-overlay-box debug-overlay-card"
              style={{
                left: `${c.cardRegion.x * 100}%`,
                top: `${c.cardRegion.y * 100}%`,
                width: `${c.cardRegion.w * 100}%`,
                height: `${c.cardRegion.h * 100}%`,
              }}
            >
              <span className="debug-overlay-label">
                C{c.slot + 1}
                {c.suit ? `·${c.suit.toUpperCase()}` : ""}
                {c.servantId !== undefined
                  ? ` · ${c.servantId}@${c.ascension ?? "?"} (${(c.faceScore ?? 0).toFixed(2)})`
                  : c.iconScore !== undefined
                    ? ` · ${c.iconScore.toFixed(2)}`
                    : ""}
              </span>
            </Box>,
          ];
          if (c.iconRegion) {
            overlays.push(
              <Box
                key={`card-icon-${c.slot}`}
                className="debug-overlay-box debug-overlay-icon"
                style={{
                  left: `${c.iconRegion.x * 100}%`,
                  top: `${c.iconRegion.y * 100}%`,
                  width: `${c.iconRegion.w * 100}%`,
                  height: `${c.iconRegion.h * 100}%`,
                }}
              />
            );
          }
          if (c.critDigitRegions) {
            c.critDigitRegions.forEach((r, idx) => {
              const read = c.critDigitReads?.[idx];
              const glyph =
                read && read.digit !== null && read.digit !== undefined
                  ? String(read.digit)
                  : "-";
              const label = read
                ? `${glyph}@${read.score.toFixed(2)}`
                : undefined;
              const kept = read?.kept ?? false;
              overlays.push(
                <Box
                  key={`card-crit-${c.slot}-${idx}`}
                  className={`debug-overlay-box debug-overlay-crit ${
                    kept ? "debug-overlay-crit-kept" : "debug-overlay-crit-miss"
                  }`}
                  style={{
                    left: `${r.x * 100}%`,
                    top: `${r.y * 100}%`,
                    width: `${r.w * 100}%`,
                    height: `${r.h * 100}%`,
                  }}
                >
                  {label && (
                    <span className="debug-overlay-label debug-overlay-crit-label">
                      {label}
                    </span>
                  )}
                </Box>
              );
            });
          }
          return overlays;
        })}
        {noblePhantasms.map((s) => (
          <Box
            key={`np-slot-${s.slot}`}
            className={`debug-overlay-box ${
              s.ready
                ? "debug-overlay-np-ready"
                : "debug-overlay-np-empty"
            }`}
            style={{
              left: `${s.cardRegion.x * 100}%`,
              top: `${s.cardRegion.y * 100}%`,
              width: `${s.cardRegion.w * 100}%`,
              height: `${s.cardRegion.h * 100}%`,
            }}
          >
            <span className="debug-overlay-label">
              NP{s.slot + 1} · {s.ready ? "ready" : "empty"} · edge{" "}
              {(s.edgeFrac * 100).toFixed(1)}% · std {s.stdBgr.toFixed(0)}
            </span>
          </Box>
        ))}
        {battleScene && (
          <>
            <Box
              key="battle-scene-region"
              className={`debug-overlay-box ${
                battleScene.scene !== null
                  ? "debug-overlay-np-ready"
                  : "debug-overlay-np-empty"
              }`}
              style={{
                left: `${battleScene.region.x * 100}%`,
                top: `${battleScene.region.y * 100}%`,
                width: `${battleScene.region.w * 100}%`,
                height: `${battleScene.region.h * 100}%`,
              }}
            >
              <span className="debug-overlay-label">
                战斗场景{" "}
                {battleScene.scene !== null && battleScene.total !== null
                  ? `${battleScene.scene}/${battleScene.total}`
                  : "未识别"}
              </span>
            </Box>
            {battleScene.anchorBox && (
              <Box
                key="battle-scene-anchor"
                className="debug-overlay-box debug-overlay-support-region"
                style={{
                  left: `${battleScene.anchorBox.x * 100}%`,
                  top: `${battleScene.anchorBox.y * 100}%`,
                  width: `${battleScene.anchorBox.w * 100}%`,
                  height: `${battleScene.anchorBox.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">
                  BATTLE 锚点 {(battleScene.anchorScore * 100).toFixed(0)}%
                </span>
              </Box>
            )}
            {battleScene.candidates.map((d, i) => {
              const isKept = battleScene.kept.some(
                (k) =>
                  k.value === d.value &&
                  Math.abs(k.region.x - d.region.x) < 1e-6 &&
                  Math.abs(k.region.y - d.region.y) < 1e-6
              );
              return (
                <Box
                  key={`battle-scene-digit-${i}`}
                  className={`debug-overlay-box ${
                    isKept
                      ? "debug-overlay-support-name-cand"
                      : "debug-overlay-support-np-cand"
                  }`}
                  style={{
                    left: `${d.region.x * 100}%`,
                    top: `${d.region.y * 100}%`,
                    width: `${d.region.w * 100}%`,
                    height: `${d.region.h * 100}%`,
                  }}
                >
                  <span className="debug-overlay-label">
                    {d.value} · {(d.score * 100).toFixed(0)}%
                    {isKept ? "" : " (drop)"}
                  </span>
                </Box>
              );
            })}
          </>
        )}
        {attackButton && (
          <>
            <Box
              key="attack-button-region"
              className={`debug-overlay-box ${
                attackButton.found
                  ? "debug-overlay-np-ready"
                  : "debug-overlay-np-empty"
              }`}
              style={{
                left: `${attackButton.region.x * 100}%`,
                top: `${attackButton.region.y * 100}%`,
                width: `${attackButton.region.w * 100}%`,
                height: `${attackButton.region.h * 100}%`,
              }}
            >
              <span className="debug-overlay-label">
                攻击按钮 · {(attackButton.score * 100).toFixed(1)}% /{" "}
                {(attackButton.threshold * 100).toFixed(0)}%{" "}
                {attackButton.found ? "✓" : "✗"}
              </span>
            </Box>
            {attackButton.matchRegion && attackButton.found && (
              <Box
                key="attack-button-match"
                className="debug-overlay-box debug-overlay-face"
                style={{
                  left: `${attackButton.matchRegion.x * 100}%`,
                  top: `${attackButton.matchRegion.y * 100}%`,
                  width: `${attackButton.matchRegion.w * 100}%`,
                  height: `${attackButton.matchRegion.h * 100}%`,
                }}
              />
            )}
            <Box
              key="attack-button-tap"
              className="debug-coord-dot"
              style={{
                left: `${attackButton.tapPoint.x * 100}%`,
                top: `${attackButton.tapPoint.y * 100}%`,
              }}
              title={`tap (${attackButton.tapPoint.x.toFixed(3)}, ${attackButton.tapPoint.y.toFixed(3)})`}
            >
              <span className="debug-coord-label">点击</span>
            </Box>
          </>
        )}
        {enhancementServantResult && (
          <>
            <Box
              key="enhancement-servant-search"
              className="debug-overlay-box debug-overlay-support-region"
              style={{
                left: `${enhancementServantResult.searchRegion.x * 100}%`,
                top: `${enhancementServantResult.searchRegion.y * 100}%`,
                width: `${enhancementServantResult.searchRegion.w * 100}%`,
                height: `${enhancementServantResult.searchRegion.h * 100}%`,
              }}
            >
              <span className="debug-overlay-label">
                强化从者 #{enhancementServantResult.servantId}
              </span>
            </Box>
            {enhancementServantResult.anchors.map((a, i) => (
              <Box
                key={`enhancement-anchor-${i}`}
                className="debug-overlay-box debug-overlay-support-name-cand"
                style={{
                  left: `${a.x * 100}%`,
                  top: `${a.y * 100}%`,
                  width: `${a.w * 100}%`,
                  height: `${a.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">
                  anchor · e{a.edgeScore.toFixed(2)} · g{a.grayScore.toFixed(2)}
                </span>
              </Box>
            ))}
            {enhancementServantResult.gridCells.map((c) => (
              <Box
                key={`enhancement-cell-${c.row}-${c.col}`}
                className="debug-overlay-box debug-overlay-card"
                style={{
                  left: `${c.region.x * 100}%`,
                  top: `${c.region.y * 100}%`,
                  width: `${c.region.w * 100}%`,
                  height: `${c.region.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">
                  r{c.row}c{c.col}
                </span>
              </Box>
            ))}
            {enhancementServantResult.best?.region ? (
                <Box
                  key="enhancement-best-face"
                  className={`debug-overlay-box ${
                    enhancementServantResult.best.found
                      ? "debug-overlay-support-name-cand"
                      : "debug-overlay-support-np-cand"
                  }`}
                  style={{
                    left: `${enhancementServantResult.best.region.x * 100}%`,
                    top: `${enhancementServantResult.best.region.y * 100}%`,
                    width: `${enhancementServantResult.best.region.w * 100}%`,
                    height: `${enhancementServantResult.best.region.h * 100}%`,
                  }}
                >
                  <span className="debug-overlay-label">
                    best r{enhancementServantResult.best.row}c
                    {enhancementServantResult.best.col} ·{" "}
                    {enhancementServantResult.best.score.toFixed(2)}
                    {enhancementServantResult.best.found ? "" : " ✗"}
                  </span>
                </Box>
            ) : null}
          </>
        )}
        {supportResult && (
          <>
            <Box
              key="support-list-region"
              className="debug-overlay-box debug-overlay-support-region"
              style={{
                left: `${supportResult.diagnostics.listRegion.x * 100}%`,
                top: `${supportResult.diagnostics.listRegion.y * 100}%`,
                width: `${supportResult.diagnostics.listRegion.w * 100}%`,
                height: `${supportResult.diagnostics.listRegion.h * 100}%`,
              }}
            >
              <span className="debug-overlay-label">助战列表</span>
            </Box>
            {supportResult.diagnostics.supportRowAnchorSearchRegion ? (
              <Box
                key="support-row-anchor-search-region"
                className="debug-overlay-box debug-overlay-support-anchor-search"
                style={{
                  left: `${supportResult.diagnostics.supportRowAnchorSearchRegion.x * 100}%`,
                  top: `${supportResult.diagnostics.supportRowAnchorSearchRegion.y * 100}%`,
                  width: `${supportResult.diagnostics.supportRowAnchorSearchRegion.w * 100}%`,
                  height: `${supportResult.diagnostics.supportRowAnchorSearchRegion.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">确认搜索</span>
              </Box>
            ) : null}
            {supportResult.diagnostics.nameCandidates.map((c, i) => (
              <Box
                key={`support-name-cand-${i}`}
                className="debug-overlay-box debug-overlay-support-name-cand"
                style={{
                  left: `${c.region.x * 100}%`,
                  top: `${c.region.y * 100}%`,
                  width: `${c.region.w * 100}%`,
                  height: `${c.region.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">
                  名 {c.score.toFixed(2)} · {c.text}
                </span>
              </Box>
            ))}
            {supportResult.diagnostics.npCandidates.map((c, i) => (
              <Box
                key={`support-np-cand-${i}`}
                className="debug-overlay-box debug-overlay-support-np-cand"
                style={{
                  left: `${c.region.x * 100}%`,
                  top: `${c.region.y * 100}%`,
                  width: `${c.region.w * 100}%`,
                  height: `${c.region.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">
                  宝 {c.score.toFixed(2)} · {c.text}
                  {c.matchedName ? ` (${c.matchedName})` : ""}
                </span>
              </Box>
            ))}
            {supportResult.supports.map((s, i) => (
              <Box
                key={`support-row-${i}`}
                className="debug-overlay-box debug-overlay-support-row"
                style={{
                  left: `${s.rowRegion.x * 100}%`,
                  top: `${s.rowRegion.y * 100}%`,
                  width: `${s.rowRegion.w * 100}%`,
                  height: `${s.rowRegion.h * 100}%`,
                }}
              >
                <span className="debug-overlay-label">
                  助战 {i + 1} · 名 {s.nameScore.toFixed(2)} · 宝{" "}
                  {s.npScore.toFixed(2)}
                  {s.npLevel != null ? ` · 宝Lv${s.npLevel}` : ""}
                  {s.skillPanel
                    ? ` · ${supportPanelShortLabel(s.skillPanel)} ${
                        s.skillPanel === "append"
                          ? supportPanelLevels(s.appendSkillLevels)
                          : supportPanelLevels(s.skillLevels)
                      }`
                    : ""}
                </span>
              </Box>
            ))}
            {supportResult.supports.map((s, i) => (
              <Box
                key={`support-tap-${i}`}
                className="debug-coord-dot debug-overlay-support-tap"
                style={{
                  left: `${s.tap.x * 100}%`,
                  top: `${s.tap.y * 100}%`,
                }}
                title={`tap (${s.tap.x.toFixed(3)}, ${s.tap.y.toFixed(3)})`}
              >
                <span className="debug-coord-label">点击 {i + 1}</span>
              </Box>
            ))}
            {supportResult.supports.map((s, i) =>
              s.scoreAnchor ? (
                <Box
                  key={`support-score-anchor-${i}`}
                  className="debug-overlay-box debug-overlay-support-score-anchor"
                  style={{
                    left: `${s.scoreAnchor.x * 100}%`,
                    top: `${s.scoreAnchor.y * 100}%`,
                    width: `${s.scoreAnchor.w * 100}%`,
                    height: `${s.scoreAnchor.h * 100}%`,
                  }}
                  title={`row anchor (${s.scoreAnchor.x.toFixed(3)}, ${s.scoreAnchor.y.toFixed(3)}, ${s.scoreAnchor.w.toFixed(3)}, ${s.scoreAnchor.h.toFixed(3)})`}
                >
                  <span className="debug-overlay-label">确认 {i + 1}</span>
                </Box>
              ) : null
            )}
            {supportResult.supports.map((s, i) =>
              s.ce ? (
                <Box
                  key={`support-ce-${i}`}
                  className="debug-overlay-box debug-overlay-support-ce"
                  style={{
                    left: `${s.ce.region.x * 100}%`,
                    top: `${s.ce.region.y * 100}%`,
                    width: `${s.ce.region.w * 100}%`,
                    height: `${s.ce.region.h * 100}%`,
                    outline: `2px solid ${s.ce.passed ? "#3fb950" : "#f85149"}`,
                  }}
                  title={
                    s.ce.error
                      ? `CE verify error: ${s.ce.error}`
                      : `CE score ${s.ce.score.toFixed(3)} (threshold ${s.ce.threshold.toFixed(2)})`
                  }
                >
                  <span className="debug-overlay-label">
                    礼 {s.ce.score.toFixed(2)}/{s.ce.threshold.toFixed(2)}{" "}
                    {s.ce.passed ? "✓" : "✗"}
                  </span>
                </Box>
              ) : null
            )}
            {supportResult.supports.flatMap((s, rowIndex) =>
              (s.grandCes ?? []).map((ce, ceIndex) => (
                <Box
                  key={`support-grand-ce-${rowIndex}-${ceIndex}`}
                  className="debug-overlay-box debug-overlay-support-ce"
                  style={{
                    left: `${ce.region.x * 100}%`,
                    top: `${ce.region.y * 100}%`,
                    width: `${ce.region.w * 100}%`,
                    height: `${ce.region.h * 100}%`,
                    outline: `2px solid ${ce.passed ? "#3fb950" : "#f85149"}`,
                  }}
                  title={
                    ce.error
                      ? `Grand CE ${ceIndex + 1} verify error: ${ce.error}`
                      : `Grand CE ${ceIndex + 1} score ${ce.score.toFixed(3)} (threshold ${ce.threshold.toFixed(2)})`
                  }
                >
                  <span className="debug-overlay-label">
                    冠{ceIndex + 1} {ce.score.toFixed(2)}/
                    {ce.threshold.toFixed(2)} {ce.passed ? "✓" : "✗"}
                  </span>
                </Box>
              ))
            )}
            {supportResult.supports.flatMap((s, rowIndex) => {
              const checks = [
                ...(s.ce?.iconChecks ?? []),
                ...(s.grandCes ?? []).flatMap((ce) => ce.iconChecks ?? []),
              ];
              return checks.map((check, checkIndex) => (
                <Box
                  key={`support-ce-icon-${rowIndex}-${checkIndex}-${check.kind}`}
                  className="debug-overlay-box debug-overlay-support-ce"
                  style={{
                    left: `${check.region.x * 100}%`,
                    top: `${check.region.y * 100}%`,
                    width: `${check.region.w * 100}%`,
                    height: `${check.region.h * 100}%`,
                    outline: `2px dashed ${check.passed ? "#3fb950" : "#f85149"}`,
                  }}
                  title={`${check.kind} ${check.templateKey} ${check.score.toFixed(3)} / ${check.threshold.toFixed(2)}`}
                >
                  <span className="debug-overlay-label">
                    {check.kind} {check.score.toFixed(2)} {check.passed ? "✓" : "✗"}
                  </span>
                </Box>
              ));
            })}
          </>
        )}
        {showCoordOverlay &&
          coordinates?.groups
            .filter((g) => visibleSet.has(g.id))
            .flatMap((g) => [
              ...g.regions.map((r) => (
                <Box
                  key={`coord-region-${g.id}-${r.label}`}
                  className="debug-coord-region"
                  style={{
                    left: `${r.region.x * 100}%`,
                    top: `${r.region.y * 100}%`,
                    width: `${r.region.w * 100}%`,
                    height: `${r.region.h * 100}%`,
                  }}
                  title={`${g.label} · ${r.label}`}
                >
                  <span className="debug-coord-label">
                    {g.label} · {r.label}
                  </span>
                </Box>
              )),
              ...g.points.map((p) => (
                <Box
                  key={`coord-dot-${g.id}-${p.label}`}
                  className="debug-coord-dot"
                  style={{
                    left: `${p.point.x * 100}%`,
                    top: `${p.point.y * 100}%`,
                  }}
                  title={`${g.label} · ${p.label} (${p.point.x.toFixed(3)}, ${p.point.y.toFixed(3)})`}
                >
                  <span className="debug-coord-label">{p.label}</span>
                </Box>
              )),
            ])}
      </Box>
    </Box>
  );
}
