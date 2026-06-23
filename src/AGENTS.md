# Frontend

## UI Rules

- Prefer Radix Themes components and props for common UI: `Button`, `IconButton`, `Card`, `Dialog`, `Select`, `DropdownMenu`, `Tabs`, `TextField`, `Checkbox`, `Switch`, and layout primitives. Add custom CSS only when Radix cannot express the required shape or spacing.
- For selectable option cards, reuse `OptionCardRadioGroup` instead of hand-rolling a `RadioGroup` plus clickable card shell. `onSelect` is only for related state initialization; the component must still commit the selected `value` through `onValueChange`.
- For battle-page section titles, reuse `SectionHeading` instead of duplicating the `.battle-section-heading` wrapper.
- Use Radix theme tokens in `src/styles/` (`var(--gray-*)`, `var(--blue-*)`, `var(--radius-*)`) instead of hard-coded colors. Any custom color should support both light and dark mode through CSS variables. Large feature styles should be split under a feature subdirectory with an `index.css` aggregator.
- Keep the FGO-inspired look abstract: restrained blue/gold/silver accents, translucent panels, fine grid or line details, and compact operational layouts. Avoid copying game UI literally or adding decorative effects that compete with the controls.
- Every custom surface, input, and button state must be checked in both light and dark mode. Avoid fixed white backgrounds unless the element is an image/art container that needs a neutral canvas.
- Preserve dense, task-focused layouts. Do not add marketing-style hero sections, nested cards, large decorative panels, or explanatory in-app text.
- User-facing text should stay Chinese. Icons should come from `@radix-ui/react-icons` when available.

## Conventions

- **State management**: React local state only (`useState`, `useEffect`, `useMemo`, `useCallback`). No external state libraries.
- **Components**: Functional components only. Shared primitives live under `src/components/`; larger feature-owned UI should live under `src/features/<feature>/` once it has a clear owner.
- **Types**: Shared interfaces live in `src/types/`. TS interfaces must stay aligned with Rust serde structs.
- **Styling**: Use `src/styles/index.css` as the single imported stylesheet entrypoint, split into feature-oriented CSS modules under `src/styles/`. Large feature styles should live in a subdirectory with an `index.css` aggregator (for example `src/styles/team/`). Prefer Radix CSS variables (`var(--gray-7)`, `var(--blue-9)`, `var(--radius-2)`, etc.). No Tailwind, CSS Modules, or styled-components.
- **FGO team member identity**: A team may contain the same servant once as an owned servant and once as a support servant. Team-related mutations must never identify members by `servantId` alone. Prefer slot/member-instance identity for operations such as update, remove, swap, ordering, and craft-essence changes; keep the support-servant flag as semantic metadata for validation, display, and support-specific behavior.

## Testing

Vitest + `@testing-library/react` + jsdom. Configured in [vite.config.ts](../vite.config.ts) under the `test` block (`include: ["src/**/__tests__/**/*.test.{ts,tsx}"]`). The shared setup (`src/test/setup.ts`) stubs `@tauri-apps/api/core` so `invoke()` resolves against an in-memory mock; component tests use `renderWithTheme` from `src/test/renderWithTheme.tsx` to mount inside a Radix `<Theme>`. **Test files live in a sibling `__tests__/` folder next to the feature or shared component code they cover** (e.g. `src/features/team/__tests__/ContentGrid.test.tsx` for `src/features/team/ContentGrid.tsx`) — never colocated alongside the component file.
