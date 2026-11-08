import type { ReactNode } from "react";

export function FooterNote({ children, className = "", role = "note" }: {
  children: ReactNode;
  className?: string;
  role?: "note" | "status" | "alert";
}) {
  return <div className={`footer-note ${className}`} role={role} data-error={role === "alert"}>
    {children && <><i aria-hidden="true" /><span>{children}</span></>}
  </div>;
}
