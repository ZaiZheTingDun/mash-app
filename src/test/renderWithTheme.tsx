import type { ReactElement } from "react";
import { render, type RenderOptions, type RenderResult } from "@testing-library/react";
import { Theme } from "@radix-ui/themes";

/**
 * Mirror the `<Theme>` wrapper from `src/main.tsx` so component tests
 * receive the same Radix CSS variables (`--gray-7`, `--blue-9`, …) that
 * production runs do. Without this, Radix components silently render
 * with empty CSS-variable fallbacks and queries against text colour /
 * sizing become flaky.
 */
export function renderWithTheme(
  ui: ReactElement,
  options?: RenderOptions
): RenderResult {
  return render(ui, {
    wrapper: ({ children }) => (
      <Theme appearance="light" accentColor="blue" radius="medium">
        {children}
      </Theme>
    ),
    ...options,
  });
}
