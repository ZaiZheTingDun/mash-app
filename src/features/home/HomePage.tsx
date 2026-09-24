import { useEffect, useRef, useState } from "react";
import { Avatar, Button, Dialog, Flex, IconButton, Text } from "@radix-ui/themes";
import {
  ArrowLeftIcon,
  CardStackIcon,
  ChevronRightIcon,
  LightningBoltIcon,
  StarIcon,
  TargetIcon,
  ThickArrowUpIcon,
} from "@radix-ui/react-icons";
import { convertFileSrc } from "../../tauri";
import { masterFacePath, masterFigurePath, type MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";

interface HomePageProps {
  mysticCodes: MysticCode[];
  figureId: number;
  gender: MysticCodeGender;
  showSummon: boolean;
  showEnhancement: boolean;
  showRankUpQuest: boolean;
  onSelectFigure: (id: number) => Promise<void>;
  onOpenTeam: () => void;
  onOpenSummon: () => void;
  onOpenCraftEssenceEnhancement: () => void;
  onOpenRankUpQuest: () => void;
}

export function HomePage({
  mysticCodes,
  figureId,
  gender,
  showSummon,
  showEnhancement,
  showRankUpQuest,
  onSelectFigure,
  onOpenTeam,
  onOpenSummon,
  onOpenCraftEssenceEnhancement,
  onOpenRankUpQuest,
}: HomePageProps) {
  const [battleMenuOpen, setBattleMenuOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [figureClip, setFigureClip] = useState<string>();
  const figurePixels = useRef<ImageData | null>(null);
  const selected = mysticCodes.find((code) => code.id === figureId);
  const imagePath = selected ? masterFigurePath(selected, gender) : null;
  const available = mysticCodes.filter((code) => code.masterFigureFemalePath && code.masterFigureMalePath);

  useEffect(() => {
    figurePixels.current = null;
    setFigureClip(undefined);
  }, [imagePath]);

  // 缓存立绘透明像素，用于将右键更换区域限制在人物轮廓内。
  const rememberFigurePixels = (image: HTMLImageElement) => {
    figurePixels.current = null;
    try {
      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      const context = canvas.getContext("2d", { willReadFrequently: true });
      if (!context) return;
      context.drawImage(image, 0, 0);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
      figurePixels.current = pixels;
      let left = pixels.width;
      let top = pixels.height;
      let right = -1;
      let bottom = -1;
      for (let y = 0; y < pixels.height; y += 1) {
        for (let x = 0; x < pixels.width; x += 1) {
          if (pixels.data[(y * pixels.width + x) * 4 + 3] < 16) continue;
          left = Math.min(left, x);
          top = Math.min(top, y);
          right = Math.max(right, x);
          bottom = Math.max(bottom, y);
        }
      }
      if (right >= left) {
        setFigureClip(`inset(${top / pixels.height * 100}% ${(pixels.width - right - 1) / pixels.width * 100}% ${(pixels.height - bottom - 1) / pixels.height * 100}% ${left / pixels.width * 100}%)`);
      }
    } catch {
      // Some asset protocols do not allow reading image pixels; the image bounds remain usable.
    }
  };

  const openFigurePicker = (event: React.MouseEvent<HTMLImageElement>) => {
    const pixels = figurePixels.current;
    if (pixels) {
      const bounds = event.currentTarget.getBoundingClientRect();
      if (bounds.width > 0 && bounds.height > 0) {
        const x = Math.floor((event.clientX - bounds.left) * pixels.width / bounds.width);
        const y = Math.floor((event.clientY - bounds.top) * pixels.height / bounds.height);
        if (x < 0 || y < 0 || x >= pixels.width || y >= pixels.height) return;
        if (pixels.data[(y * pixels.width + x) * 4 + 3] < 16) return;
      }
    }
    event.preventDefault();
    setPickerOpen(true);
  };

  const chooseFigure = async (id: number) => {
    setSaving(true);
    setSaveError(null);
    try {
      await onSelectFigure(id);
      setPickerOpen(false);
    } catch (error) {
      setSaveError(`保存立绘失败：${String(error)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <main className="home-page">
      <div className="home-panel">
        <div className="home-figure-area">
          <div className="home-figure-trigger">
            {imagePath ? (
              <img
                src={convertFileSrc(imagePath)}
                crossOrigin="anonymous"
                alt={selected?.name ?? "御主立绘"}
                title="右键更换御主立绘"
                tabIndex={0}
                style={figureClip ? { clipPath: figureClip } : undefined}
                onLoad={(event) => rememberFigurePixels(event.currentTarget)}
                onContextMenu={openFigurePicker}
                onKeyDown={(event) => {
                  if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
                    event.preventDefault();
                    setPickerOpen(true);
                  }
                }}
              />
            ) : (
              <span
                className="home-figure-placeholder"
                title="右键更换御主立绘"
                onContextMenu={(event) => {
                  event.preventDefault();
                  setPickerOpen(true);
                }}
              >未找到御主立绘，请更新资源包</span>
            )}
          </div>
        </div>

        <nav className="home-actions" aria-label={battleMenuOpen ? "战斗菜单" : "主页菜单"}>
          <div className="home-menu-heading">
            <span className="home-menu-kicker">MASH</span>
            <div className="home-menu-title-row">
              {battleMenuOpen && (
                <IconButton
                  className="home-menu-back"
                  variant="ghost"
                  color="gray"
                  aria-label="返回"
                  onClick={() => setBattleMenuOpen(false)}
                >
                  <ArrowLeftIcon />
                </IconButton>
              )}
              <h1>{battleMenuOpen ? "战斗" : "选择任务"}</h1>
            </div>
          </div>
          {battleMenuOpen ? (
            <>
              <button type="button" className="home-action home-action--battle" aria-label="编队/开始" onClick={onOpenTeam}>
                <span className="home-action-icon"><CardStackIcon aria-hidden="true" /></span>
                <span className="home-action-copy"><strong>编队/开始</strong><small>选择队伍并开始战斗</small></span>
                <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
              </button>
              {showRankUpQuest && (
                <button type="button" className="home-action home-action--rank-up" aria-label="强化任务" onClick={onOpenRankUpQuest}>
                  <span className="home-action-icon"><LightningBoltIcon aria-hidden="true" /></span>
                  <span className="home-action-copy"><strong>强化任务</strong><small>进入强化任务</small></span>
                  <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
                </button>
              )}
            </>
          ) : (
            <>
              <button type="button" className="home-action home-action--battle" aria-label="战斗" onClick={() => setBattleMenuOpen(true)}>
                <span className="home-action-icon"><TargetIcon aria-hidden="true" /></span>
                <span className="home-action-copy"><strong>战斗</strong><small>编队、开始与强化任务</small></span>
                <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
              </button>
              {showSummon && (
                <button type="button" className="home-action home-action--summon" aria-label="召唤" onClick={onOpenSummon}>
                  <span className="home-action-icon"><StarIcon aria-hidden="true" /></span>
                  <span className="home-action-copy"><strong>召唤</strong><small>友情点召唤</small></span>
                  <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
                </button>
              )}
              {showEnhancement && (
                <button type="button" className="home-action home-action--enhance" aria-label="强化" onClick={onOpenCraftEssenceEnhancement}>
                  <span className="home-action-icon"><ThickArrowUpIcon aria-hidden="true" /></span>
                  <span className="home-action-copy"><strong>强化</strong><small>概念礼装强化</small></span>
                  <ChevronRightIcon className="home-action-arrow" aria-hidden="true" />
                </button>
              )}
            </>
          )}
        </nav>
      </div>

      <Dialog.Root open={pickerOpen} onOpenChange={setPickerOpen}>
        <Dialog.Content className="home-figure-dialog">
          <Dialog.Title>更换御主立绘</Dialog.Title>
          <Dialog.Description size="2" color="gray">只更换主页立绘，不影响队伍的御主礼装。</Dialog.Description>
          {available.length === 0 ? (
            <Text size="2" color="gray">未找到可用立绘，请更新资源包。</Text>
          ) : (
            <div className="home-figure-options">
              {available.map((code) => {
                const face = masterFacePath(code, gender);
                return (
                  <button
                    type="button"
                    key={code.id}
                    className="home-figure-option"
                    aria-label={code.name}
                    aria-pressed={code.id === figureId}
                    disabled={saving}
                    onClick={() => void chooseFigure(code.id)}
                  >
                    <Avatar src={face ? convertFileSrc(face) : undefined} fallback="御" size="5" />
                    <span>{code.name}</span>
                  </button>
                );
              })}
            </div>
          )}
          {saveError && <Text size="2" color="red">{saveError}</Text>}
          <Flex justify="end" mt="3">
            <Button variant="soft" color="gray" onClick={() => setPickerOpen(false)}>关闭</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </main>
  );
}
