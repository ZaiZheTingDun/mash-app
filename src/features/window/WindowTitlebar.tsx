import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Dialog, DropdownMenu, Flex, Text } from "@radix-ui/themes";
import { Cross1Icon, HamburgerMenuIcon, MinusIcon, SquareIcon, CopyIcon } from "@radix-ui/react-icons";
import { emit, invoke, listen } from "../../tauri";
import type { WindowChrome, WindowChromeAction } from "../../types/windowChrome";
import appIcon from "../../../src-tauri/icons/32x32.png";

export function WindowTitlebar({ ready }: { ready: boolean }) {
  const [chrome, setChrome] = useState<WindowChrome | null>(null);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);

  const refresh = useCallback(async () => {
    const current = ++request.current;
    const next = await invoke<WindowChrome>("get_window_chrome");
    if (current === request.current) setChrome(next);
  }, []);

  useEffect(() => {
    const current = ++request.current;
    void invoke<WindowChrome>("get_window_chrome")
      .then((next) => { if (current === request.current) setChrome(next); })
      .catch((err: unknown) => console.error("Failed to load window chrome", err));
    const unlisten = listen("tauri://resize", () => {
      void refresh().catch((err: unknown) => console.error("Failed to refresh window chrome", err));
    }).catch((err: unknown) => {
      console.error("Failed to listen for window resize", err);
      return () => {};
    });
    return () => {
      request.current += 1;
      void unlisten.then((stop) => stop());
    };
  }, [refresh]);

  const run = (operation: () => Promise<unknown>) => {
    void operation().catch((err: unknown) => setError(String(err)));
  };

  const control = (action: WindowChromeAction) => run(async () => {
    await invoke("window_chrome_action", { action });
    if (action === "toggleMaximize") await refresh();
  });

  if (!chrome?.customTitlebar) return null;

  const maximizeLabel = chrome.maximized ? "还原窗口" : "最大化";
  return (
    <>
      <header className="window-titlebar" aria-label="窗口标题栏">
        <div
          className="window-titlebar-drag"
          onMouseDown={(event) => {
            if (event.button !== 0 || (event.detail !== 1 && event.detail !== 2)) return;
            event.preventDefault();
            // Native dragging can consume mouseup, so handle the second press here.
            control(event.detail === 2 ? "toggleMaximize" : "startDragging");
          }}
        >
          <img src={appIcon} alt="" width="18" height="18" draggable={false} />
          <span className="window-titlebar-name">{chrome.title}</span>
        </div>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            <button className="window-titlebar-button window-titlebar-menu" aria-label="应用菜单" title="应用菜单">
              <HamburgerMenuIcon />
            </button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Content size="1" align="end">
            <DropdownMenu.Item disabled={!ready} onSelect={() => run(() => emit("save-adb-screenshot-requested"))}>截图…</DropdownMenu.Item>
            <DropdownMenu.Item disabled={!ready} onSelect={() => run(() => emit("device-resolution-requested"))}>重置设备分辨率…</DropdownMenu.Item>
            <DropdownMenu.Item disabled={!ready} onSelect={() => run(() => emit("self-check-requested"))}>自检…</DropdownMenu.Item>
            <DropdownMenu.Item disabled={!ready} onSelect={() => run(() => emit("resource-manager-requested"))}>资源管理…</DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item disabled={!ready} onSelect={() => run(() => emit("updater-check-requested"))}>检查更新…</DropdownMenu.Item>
            <DropdownMenu.Item onSelect={() => setAboutOpen(true)}>关于 Mash</DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item onSelect={() => control("close")}>关闭窗口</DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
        <div className="window-titlebar-controls">
          <button className="window-titlebar-button" aria-label="最小化" title="最小化" onClick={() => control("minimize")}><MinusIcon /></button>
          <button className="window-titlebar-button" aria-label={maximizeLabel} title={maximizeLabel} onClick={() => control("toggleMaximize")}>
            {chrome.maximized ? <CopyIcon /> : <SquareIcon />}
          </button>
          <button className="window-titlebar-button window-titlebar-close" aria-label="关闭窗口" title="关闭窗口" onClick={() => control("close")}><Cross1Icon /></button>
        </div>
      </header>
      <Dialog.Root open={aboutOpen} onOpenChange={setAboutOpen}>
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>关于 Mash</Dialog.Title>
          <Dialog.Description>版本 {chrome.version}</Dialog.Description>
          <Flex justify="end" mt="4"><Dialog.Close><Button>关闭</Button></Dialog.Close></Flex>
        </Dialog.Content>
      </Dialog.Root>
      <Dialog.Root open={error != null} onOpenChange={(open) => { if (!open) setError(null); }}>
        <Dialog.Content maxWidth="400px">
          <Dialog.Title>窗口操作失败</Dialog.Title>
          <Dialog.Description><Text>{error}</Text></Dialog.Description>
          <Flex justify="end" mt="4"><Dialog.Close><Button>关闭</Button></Dialog.Close></Flex>
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}
