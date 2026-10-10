export function SectionRailIcon({ className = "", stem = true }: { className?: string; stem?: boolean }) {
  return <svg className={`section-rail-icon ${className}`} viewBox="0 0 12 28" aria-hidden="true"><path d={`M6 9 11 14 6 19 1 14Z${stem ? " M6 19V27" : ""}`} fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>;
}
