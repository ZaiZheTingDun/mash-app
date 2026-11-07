import { Button, DropdownMenu, Flex, IconButton, Popover, Skeleton, Spinner, Text } from "@radix-ui/themes";
import { CheckIcon, ChevronDownIcon, DotsHorizontalIcon, ReloadIcon } from "@radix-ui/react-icons";
import { useState } from "react";
import type { usePacketCapture } from "./usePacketCapture";

export function CaptureClientPicker({ capture }: { capture: ReturnType<typeof usePacketCapture> }) {
  const [open, setOpen] = useState(false);
  const clients = capture.clients;
  const disabled = capture.busy || capture.clientOperation === "select" || capture.canStop || capture.initialLoading;
  return <div className="project-bar is-page-context">
    <Flex align="center" gap="2" className="project-bar-controls">
      <div className="project-context-selector">
        <span className="project-context-label">CLIENT</span>
        <Popover.Root open={open} onOpenChange={next => {
          if (capture.busy || capture.clientOperation === "select") return;
          setOpen(next);
          if (next) void capture.loadClients();
        }}>
          <Popover.Trigger>
            <Button type="button" variant="surface" color="gray" size="3" className="project-bar-pill"
              disabled={disabled} aria-label="选择客户端"
              title={clients?.selectedPackage ?? undefined}>
              <Text size="3" weight="bold" className="project-bar-title">{clients?.selectedPackage ?? "选择客户端"}</Text>
              <ChevronDownIcon width={15} height={15} className="project-bar-menu-icon" />
            </Button>
          </Popover.Trigger>
          <Popover.Content className="project-selector-popover" width="360px" aria-label="抓包客户端">
            {capture.clientOperation === "load" && !clients ? <Skeleton height="36px" aria-label="正在读取客户端" /> : clients?.apps.length ? <div className="project-selector-list">
              {clients.apps.map(app => <button key={app.packageName} type="button" className="project-selector-project"
                aria-label={`${app.name} · ${app.packageName}`} aria-pressed={app.packageName === clients.selectedPackage}
                data-active={app.packageName === clients.selectedPackage} disabled={capture.busy || capture.clientBusy}
                onClick={() => void capture.chooseClient(app.packageName).then(chosen => { if (chosen) setOpen(false); })}>
                <span className="project-selector-check">{app.packageName === clients.selectedPackage && <CheckIcon width={14} height={14} />}</span>
                <Text size="2" className="project-selector-project-name" title={`${app.name} · ${app.packageName}`}>{app.name} · {app.packageName}</Text>
              </button>)}
            </div> : clients && <Text as="p" size="2" color="gray">未检测到当前服务器的 FGO 客户端</Text>}
            {capture.clientError && <Text as="p" role="alert" size="2" color="red" mt="3">{capture.clientError}</Text>}
          </Popover.Content>
        </Popover.Root>
      </div>
      <DropdownMenu.Root onOpenChange={next => { if (next) setOpen(false); }}>
        <DropdownMenu.Trigger>
          <IconButton type="button" size="3" variant="surface" color="gray" aria-label="客户端操作" className="project-action-button" disabled={disabled}>
            <DotsHorizontalIcon width={16} height={16} />
          </IconButton>
        </DropdownMenu.Trigger>
        <DropdownMenu.Content>
          <DropdownMenu.Item disabled={disabled || capture.clientBusy} onSelect={() => void capture.loadClients()}>
            <Flex align="center" gap="2">
              {capture.clientOperation === "load" ? <Spinner size="1" /> : <ReloadIcon width={12} height={12} />}
              <Text size="2">刷新</Text>
            </Flex>
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </Flex>
  </div>;
}
