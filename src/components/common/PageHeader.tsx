import type { ReactNode } from "react";
import { IconButton } from "@radix-ui/themes";
import { ArrowLeftIcon } from "@radix-ui/react-icons";

export function PageHeader({ title, english, onBack, backLabel = "返回", backDisabled = false, children }: { title: string; english: string; onBack?: () => void; backLabel?: string; backDisabled?: boolean; children: ReactNode }) {
  return <header className="mash-page-header">
    {onBack && <IconButton type="button" variant="ghost" color="gray" aria-label={backLabel} disabled={backDisabled} onClick={onBack}><ArrowLeftIcon /></IconButton>}
    <div className="mash-page-project">{children}</div>
    <div className="mash-page-title"><div className="mash-page-identity"><div><small>{english}</small><h1>{title}</h1></div><svg className="mash-page-identity-icon" viewBox="0 0 24 66" aria-hidden="true"><path d="M12 0 24 24 12 48 0 24Z M12 18 24 42 12 66 0 42Z" fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" /></svg></div><div className="mash-page-title-accent" aria-hidden="true"><i /><svg viewBox="0 0 8 8"><path d="M4 0 8 4 4 8 0 4Z" fill="none" stroke="currentColor" vectorEffect="non-scaling-stroke" /></svg><i /></div></div>
  </header>;
}
