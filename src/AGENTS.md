# Frontend UI Rules

- Prefer Radix Themes components and props for common UI: `Button`, `IconButton`, `Card`, `Dialog`, `Select`, `DropdownMenu`, `Tabs`, `TextField`, `Checkbox`, `Switch`, and layout primitives. Add custom CSS only when Radix cannot express the required shape or spacing.
- For selectable option cards, reuse `OptionCardRadioGroup` instead of hand-rolling a `RadioGroup` plus clickable card shell. `onSelect` is only for related state initialization; the component must still commit the selected `value` through `onValueChange`.
- For battle-page section titles, reuse `SectionHeading` instead of duplicating the `.battle-section-heading` wrapper.
- Use Radix theme tokens in `src/styles/` (`var(--gray-*)`, `var(--blue-*)`, `var(--radius-*)`) instead of hard-coded colors. Any custom color should support both light and dark mode through CSS variables.
- Keep the FGO-inspired look abstract: restrained blue/gold/silver accents, translucent panels, fine grid or line details, and compact operational layouts. Avoid copying game UI literally or adding decorative effects that compete with the controls.
- Every custom surface, input, and button state must be checked in both light and dark mode. Avoid fixed white backgrounds unless the element is an image/art container that needs a neutral canvas.
- Preserve dense, task-focused layouts. Do not add marketing-style hero sections, nested cards, large decorative panels, or explanatory in-app text.
- User-facing text should stay Chinese. Icons should come from `@radix-ui/react-icons` when available.
