# Design System

> The authoritative, exhaustive inventory of every visual token allowed in Project Builder. **Before writing any component, check this file.** If you need a value that isn't listed here, stop and open a separate task to add it — never invent.

## Status & philosophy

- **Dark theme only.** There is no light mode and no plans for one. All tokens assume a dark background.
- **Tailwind CSS 4, utility-first.** Styling lives in `className`. There is no `tailwind.config`, no `@theme` block, no `theme.ts`, no CSS variables, no CSS modules, no styled-components. The only CSS file is `src/index.css` and it contains `@import "tailwindcss";` plus root height reset.
- **This file enumerates the allowed set.** Every value below was extracted from the existing codebase (`src/components/**/*.tsx`). Nothing here is aspirational.
- **To extend**: propose the new token in a task, update this file *first*, then write the component. Never the reverse.
- **Match reality.** If you find a conflict between this file and the codebase, the codebase wins and this file is wrong — fix this file and flag the discrepancy.

---

## Surfaces (backgrounds)

Solid surfaces, darkest → lightest:

| Class | Role |
|---|---|
| `bg-gray-950` | Darkest background — app shell, root panels |
| `bg-gray-900` | Dark container — cards, side panels, chat bubbles |
| `bg-gray-800` | Raised element — hovered rows, active pills, inputs |
| `bg-gray-700` | Lightest raised surface — buttons, dividers, highlights |
| `bg-black` | True black — rare, only for deepest overlay contrast |
| `bg-transparent` | Explicit clear |

Overlay surfaces (semi-transparent):

| Class | Role |
|---|---|
| `bg-gray-900/50`, `bg-gray-900/60`, `bg-gray-900/70`, `bg-gray-900/80` | Muted panel over canvas / content |
| `bg-gray-800/50` | Muted raised element over canvas |
| `bg-black/40`, `bg-black/50`, `bg-black/60` | Modal / dialog scrims |

---

## Text colors

| Class | Role |
|---|---|
| `text-white` | Maximum contrast — button labels on colored backgrounds |
| `text-gray-100` | Primary text — headings, important body |
| `text-gray-200` | Secondary primary — body paragraphs, labels |
| `text-gray-300` | Secondary — supporting text |
| `text-gray-400` | Muted — captions, inactive tabs, helper text |
| `text-gray-500` | Disabled / very muted |
| `text-gray-600` | Placeholders, decorative |

---

## Borders

Neutral:

| Class | Role |
|---|---|
| `border-gray-600` | Strongest neutral divider |
| `border-gray-700` | Default border — cards, inputs, dividers |
| `border-gray-800` | Subtle divider between related elements |

Semantic borders are listed in each color section below.

---

## Semantic palette

Each semantic color follows the same pattern: a solid button background, optional darker variant for hover, a text shade, a border shade, and a tinted background for status banners.

### Blue — primary / info

| Purpose | Class |
|---|---|
| Primary button background | `bg-blue-600` |
| Primary button hover | `bg-blue-500` |
| Text on colored bg | `text-white` |
| Info text | `text-blue-200`, `text-blue-300`, `text-blue-400` |
| Link text | `text-blue-400 hover:underline` |
| Border (strong) | `border-blue-700`, `border-blue-800` |
| Border (muted) | `border-blue-900/60` |
| Status banner bg | `bg-blue-900/30`, `bg-blue-950/30` |
| Focus ring | `focus:border-blue-500` |

### Green — success / run actions

| Purpose | Class |
|---|---|
| Success button bg | `bg-green-600` |
| Text | `text-green-200`, `text-green-300`, `text-green-400` |
| Border | `border-green-700`, `border-green-900/60` |
| Status banner bg | `bg-green-900/40`, `bg-green-950/30` |

### Emerald — alternative success

Used alongside green when a visual distinction is needed (e.g. validation success vs. run success).

| Purpose | Class |
|---|---|
| Button bg | `bg-emerald-600` |
| Text | `text-emerald-200`, `text-emerald-300` |
| Border | `border-emerald-900/50` |
| Banner bg | `bg-emerald-900/40`, `bg-emerald-950/20` |

### Red — destructive / error

| Purpose | Class |
|---|---|
| Destructive button bg | `bg-red-600` |
| Destructive hover | `bg-red-500`, `bg-red-700` |
| Text | `text-red-200`, `text-red-300`, `text-red-400` |
| Border | `border-red-700`, `border-red-800`, `border-red-900/60` |
| Banner bg | `bg-red-900/20`, `bg-red-900/50`, `bg-red-950/40`, `bg-red-950/60` |
| Toast bg | `bg-red-900/90` |

### Amber — warning

| Purpose | Class |
|---|---|
| Text | `text-amber-100`, `text-amber-200`, `text-amber-300`, `text-amber-400` |
| Border | `border-amber-600`, `border-amber-700`, `border-amber-900/50` |
| Banner bg | `bg-amber-900/20`, `bg-amber-950/20`, `bg-amber-950/90` |
| Toast bg | `bg-amber-900/90` |

### Yellow — accent

Used sparingly for highlighted tokens inside other content.

| Purpose | Class |
|---|---|
| Text | `text-yellow-500` |

### Purple — secondary

| Purpose | Class |
|---|---|
| Button bg | `bg-purple-600`, `bg-purple-700` |
| Text | `text-purple-200`, `text-purple-300` |

---

## Status-banner pattern

The canonical pattern for inline status messages is a triplet: tinted background + strong border + bright text.

```
bg-{color}-900/30  border border-{color}-700  text-{color}-300
```

| Status | Color |
|---|---|
| Info | `blue` |
| Success | `green` (or `emerald` when contrasting with green elsewhere) |
| Warning | `amber` |
| Error | `red` |

Reuse this pattern instead of inventing new banner styles.

---

## Spacing scale

Use the Tailwind default scale. The in-use subset (for consistency, stay within this unless you have a concrete reason):

`0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 5, 6, 8`

Larger values appear in specific layout contexts only: `mt-20` (empty-state spacing), `space-y-8` (top-level section spacing). Don't introduce new large-gap values without a reason.

Applies to all of `p-*`, `px-*`, `py-*`, `pt-*`, `pl-*`, `m-*`, `mx-*`, `my-*`, `mb-*`, `mt-*`, `gap-*`, `gap-x-*`, `space-y-*`.

**Do not use `p-[Npx]` or similar arbitrary spacing values.** If the scale is wrong for your case, flag it.

---

## Sizing

Named values in use:

- **Widths**: `w-full`, `w-px`, `w-1.5`, `w-4`, `w-5`, `w-7`, `w-20`, `w-24`, `w-32`, `w-40`, `w-48`, `w-72`, `w-96`
- **Heights**: `h-full`, `h-1.5`, `h-4`, `h-5`, `h-7`
- **Max widths**: `max-w-xs`, `max-w-sm`, `max-w-xl`, `max-w-2xl`, `max-w-3xl`
- **Max heights**: `max-h-24`, `max-h-28`, `max-h-32`, `max-h-40`, `max-h-48`, `max-h-64`, `max-h-72`
- **Min widths**: `min-w-0` (for flex-child truncation)

### Approved arbitrary size values

These exist because Tailwind's scale doesn't cover them and the use case is stable. **Reuse these exact values; do not invent new arbitrary widths/heights.**

| Value | Use for |
|---|---|
| `w-[28rem]` | Wide fixed side panels (CTO chat, Leader plan) |
| `h-[70vh]` | Modal / dialog body max height |
| `max-w-[120px]` | Narrow truncation cell |
| `max-w-[160px]` | Medium truncation cell |

---

## Border radius

Only these four values:

| Class | Use for |
|---|---|
| `rounded` | Default — inputs, small chips |
| `rounded-lg` | Cards, containers, dialogs |
| `rounded-xl` | Large containers / prominent cards |
| `rounded-full` | Pills, badges, avatar placeholders |

No arbitrary radii.

---

## Shadows

Only these three values:

| Class | Use for |
|---|---|
| `shadow-lg` | Small raised elements |
| `shadow-xl` | Dialogs, dropdowns |
| `shadow-2xl` | Main panels, primary overlays |

No arbitrary shadows.

---

## Typography

### Size

| Class | Role |
|---|---|
| `text-xs` | **Dominant UI size** — labels, body, buttons, most text |
| `text-sm` | Section headings, prominent labels |
| `text-base` | Larger body (rare) |
| `text-lg` | Page titles |

Approved arbitrary pixel sizes (for compact UI; reuse, don't invent new px sizes):

- `text-[9px]`
- `text-[10px]`
- `text-[11px]`

### Weight

| Class | Role |
|---|---|
| `font-normal` (implicit) | Body text |
| `font-medium` | Buttons, strong labels |
| `font-semibold` | Section headings |
| `font-bold` | Page titles, emphasis |

### Family

| Class | Role |
|---|---|
| Default (sans) | Everything |
| `font-mono` | Code, terminal output, technical identifiers |

### Line height

| Class | Role |
|---|---|
| Default | UI text |
| `leading-relaxed` | Markdown body, long-form content |

### Tracking

| Class | Role |
|---|---|
| `tracking-wide` | All-caps labels |
| `tracking-wider` | Optional stronger emphasis on all-caps |

### Alignment

`text-left`, `text-center`, `text-right`. Default is `text-left`.

---

## Z-index

Only these three layers:

| Class | Layer |
|---|---|
| `z-10` | Overlay above content (sticky headers, dropdowns) |
| `z-20` | Secondary overlay |
| `z-50` | Modals, toasts, top-most fixed panels |

**No arbitrary z-indexes.** If you need another layer, this file gets updated first.

---

## Component primitives

Located in `src/components/ui/`. **Use these instead of re-rolling equivalent elements.** If a primitive doesn't cover your case, extend the primitive — don't fork its styles inline.

### ConfirmDialog (`src/components/ui/ConfirmDialog.tsx`)
- Container: `rounded-lg border border-gray-700 bg-gray-900 p-5 shadow-xl`
- Cancel button: `rounded px-3 py-1.5 text-xs text-gray-400 hover:bg-gray-800 border border-gray-700`
- Confirm (destructive) button: `rounded bg-red-600 px-3 py-1.5 text-xs text-white hover:bg-red-500`

### ToastContainer (`src/components/ui/ToastContainer.tsx`)
- Container: `fixed bottom-4 right-4 z-50 flex flex-col gap-2`
- Error: `bg-red-900/90 text-red-100 border border-red-700`
- Warning: `bg-amber-900/90 text-amber-100 border border-amber-700`
- Info: `bg-gray-800/90 text-gray-100 border border-gray-700`

### PillSelect (`src/components/ui/PillSelect.tsx`)
- Active: `bg-blue-600 text-white border-transparent rounded-full px-3 py-1 text-xs font-medium`
- Inactive: `bg-gray-800 text-gray-400 hover:text-gray-200 border-gray-700 rounded-full px-3 py-1 text-xs font-medium`

### SelectWithOther (`src/components/ui/SelectWithOther.tsx`)
- Preset active: `bg-blue-600 text-white border-transparent`
- Preset inactive: `bg-gray-800 text-gray-400 hover:text-gray-200 border-gray-700`
- Input: `w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none`

### Markdown (`src/components/ui/Markdown.tsx`)
React-markdown wrapper for streamed LLM output. Its element class map is the canonical style for rendered-markdown content:
- Paragraph: `text-xs text-gray-200 leading-relaxed mb-1.5`
- H1: `text-sm font-bold text-gray-100 mb-1 mt-2`
- H2: `text-xs font-bold text-gray-100 mb-1 mt-2`
- H3: `text-xs font-semibold text-gray-200 mb-1 mt-1.5`
- Lists: `list-disc list-inside text-xs text-gray-200 mb-1.5 space-y-0.5 pl-1`
- Inline code: `rounded bg-gray-900 px-1 py-0.5 text-[10px] font-mono text-blue-300`
- Code block: `rounded bg-gray-900 p-1.5 text-[10px] font-mono overflow-x-auto mb-1.5`
- Link: `text-blue-400 hover:underline`
- Table header: `border border-gray-700 bg-gray-800 px-1.5 py-0.5 text-left font-semibold`
- Table cell: `border border-gray-700 px-1.5 py-0.5`
- Blockquote: `border-l-2 border-gray-600 pl-2 text-xs text-gray-400 italic mb-1.5`

Any change to LLM-rendered output styling should happen here, not in consumers.

---

## Breakpoints

Tailwind defaults: `sm` 640, `md` 768, `lg` 1024, `xl` 1280, `2xl` 1536.

Project Builder is a desktop Tauri app — the canvas, multi-tab editor, and side panels assume a desktop viewport. `CLAUDE.md` mandates mobile-first as a layout discipline, but not as a deployment target. Build for desktop first; use breakpoints only when a specific panel needs to collapse.

---

## What is intentionally NOT here

If you're looking for any of these and don't find them, it's because they're disallowed or not used:

- Light theme / light mode variants
- CSS-in-JS, styled-components, emotion
- CSS modules
- Custom shadows (`shadow-[...]`)
- Arbitrary z-indexes (`z-[...]`)
- Arbitrary spacing (`p-[Npx]`, `m-[Npx]`)
- Custom color values (`bg-[#hex]`, `text-[rgb(...)]`)
- Custom font families beyond default sans + `font-mono`
- Animations / transitions (none are currently tokenized; add here if introduced)
- Gradient backgrounds

---

## How to extend this file

1. Propose the new token as a **separate task** with the use case that requires it.
2. Update this file first, adding the new token to the relevant section with its role.
3. Commit the doc change.
4. Write the component that uses it.

Never the reverse. A component using an undocumented token is a bug.
