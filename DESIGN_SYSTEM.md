# Design System

> Tokens used in the UI. Source of truth is the Tailwind config (`tailwind.config.*`) and `src/index.css`. **If a token is missing here, do not invent one — add it to Tailwind first, document it here, then use it.**

## Status

> ⚠️ This document is currently a stub. The codebase uses Tailwind CSS 4.2 utility classes directly with no extracted token layer. Before designing new components, the team should extract the in-use color/spacing/radius/shadow values into named tokens in `tailwind.config` and mirror them here.

## Theme
- **Dark theme** is the default and only theme today
- Applied via Tailwind utility classes (e.g. `bg-neutral-900`, `text-neutral-100`)

## Colors
TBD — extract from existing components in `src/components/` and add to Tailwind config.

## Typography
TBD — currently using Tailwind defaults.

## Spacing
TBD — Tailwind default scale (0.25rem increments).

## Radii
TBD.

## Shadows
TBD.

## Breakpoints
- Tailwind defaults (`sm` 640, `md` 768, `lg` 1024, `xl` 1280, `2xl` 1536)
- **Mobile-first is mandated** by `CLAUDE.md`, but in practice Project Builder is a desktop Tauri app — the canvas, side panels, and multi-tab editor are designed for desktop. Treat mobile-first as a layout discipline, not a deployment target.

## Component primitives
Located in `src/components/ui/`:
- `Button`
- `Markdown` (react-markdown wrapper for LLM output, dark-theme styled)
- Other shared primitives — see directory

Use these instead of raw `<button>` / `<div>` where they exist.
