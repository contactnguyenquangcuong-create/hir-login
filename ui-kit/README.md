# Shardx UI Kit

A React + **Tailwind CSS v4** component library built from the Proxy Shard
[Figma design system](https://www.figma.com/design/g5IfoiRRLgixAfvkklpVW6/UI-KIT).
Colours, typography and components are ported from the Figma (an AlignUI-based
token system). Every component is **light / dark theme aware** and lives in its
own folder.

## Tech stack

- [React 19](https://react.dev/) + [Vite](https://vite.dev/)
- [Tailwind CSS v4](https://tailwindcss.com/) via `@tailwindcss/vite`
- TypeScript, ESLint
- Zero runtime UI dependencies (icons + `cn` helper are built in)

## Getting started

```bash
npm install
npm run dev      # showcase at http://localhost:5173
npm run build    # type-check + production build
npm run preview
npm run lint
```

## Project structure

```
src/
├─ icons/                 # 3,229 Remix icons as SVGR components (see below)
├─ components/            # one folder per component
│  ├─ Button/  Badge/  Tag/  Alert/  Input/  Textarea/
│  ├─ Checkbox/  Radio/  Switch/  Select/  Slider/
│  ├─ Tabs/  SegmentControl/  Breadcrumb/  ProgressBar/
│  ├─ Tooltip/  Modal/
│  └─ index.ts           # barrel export
├─ theme/                # ThemeProvider, useTheme, ThemeToggle
├─ styles/globals.css    # design tokens + typography (from Figma)
├─ lib/                  # cn() + inline icon set
├─ demo/                 # showcase-only helpers
├─ App.tsx               # living component gallery
└─ index.ts              # public package entry
```

## Components

| Category | Components |
| --- | --- |
| Actions | `Button` (primary/neutral/error × filled/stroke/lighter/ghost × 4 sizes) |
| Status | `Badge`, `Tag`, `Alert`, `ProgressBar` |
| Forms | `Input`, `Textarea`, `Select`, `Checkbox`, `Radio`, `Switch`, `Slider` |
| Navigation | `Tabs`, `SegmentControl`, `Breadcrumb` |
| Overlays | `Modal`, `Tooltip` |

```tsx
import { Button, Badge, Input, ThemeProvider } from '@/components'

<Button variant="primary" mode="filled">Save</Button>
<Badge color="success" variant="light" dot>Active</Badge>
<Input label="Email" placeholder="you@example.com" />
```

## Design tokens

`src/styles/globals.css` holds the full token layer:

- **Static palette** in Tailwind's `@theme` → utilities like `bg-primary-base`,
  `text-error-base`, `bg-success-weak`, `text-neutral-600`.
- **Semantic, theme-aware** CSS variables remapped per `data-theme`:
  `--bg-*`, `--text-*`, `--stroke-*`, `--icon-*` (used as `bg-bg-white-0`,
  `text-text-sub-600`, `ring-stroke-soft-200`, …).
- **Typography** classes: `text-title-h1…h6`, `text-label-*`,
  `text-paragraph-*`, `text-subheading-*` (Inter).

## Icons

The Figma Icons page is the **Remix Icon** set — all **3,229 icons** are vendored
and imported as React components via [SVGR](https://react-svgr.com/)
(`vite-plugin-svgr`). Each keeps `fill="currentColor"`, so **colour follows the
theme** (and any `text-*` utility) automatically.

```tsx
import { RiSearchLine, RiShieldCheckLine } from '@/icons'

<RiSearchLine className="size-5 text-text-sub-600" />   // adapts to theme
<RiShieldCheckLine className="size-6 text-primary-base" />

// or import any SVG file directly:
import Home from '@/icons/svg/home-5-line.svg?react'
<Home className="size-5" />
```

Names follow Remix in PascalCase with an `Ri` prefix (`close-line` → `RiCloseLine`).
Icons are side-effect-free, so unused ones are tree-shaken from your bundle.
See [src/icons/README.md](./src/icons/README.md) and the icon
[LICENSE](./src/icons/LICENSE).

## Theming

```tsx
import { ThemeProvider, useTheme, ThemeToggle } from '@/theme/ThemeProvider'

<ThemeProvider>
  <App />
</ThemeProvider>
```

`ThemeProvider` toggles `data-theme="light" | "dark"` on `<html>`, supports a
`system` option, persists to `localStorage`, and follows the OS preference on
first load. Drop in `<ThemeToggle />` for a ready-made switch.

## Building the library

```bash
npm run build:lib
```

Emits a consumable package to `dist/`:

- `dist/index.js` + `dist/index.d.ts` — components, `ThemeProvider`, `cn`
- `dist/icons/index.js` + `dist/icons/index.d.ts` — all icons
- `dist/styles.css` — compiled tokens + utilities (import once)
- per-component and per-icon modules → **tree-shakeable**; React stays a peer dep

## Publishing to GitHub Packages (private)

1. Replace `your-github-username` with your GitHub login/org in **`package.json`**
   (`name`, `repository.url`) and **`.npmrc`**:

   ```bash
   sed -i '' 's/your-github-username/YOUR_LOGIN/g' package.json .npmrc
   ```

2. Authenticate npm to GitHub Packages. Create a GitHub **Personal Access Token
   (classic)** with `write:packages` + `read:packages`, then add to your user
   `~/.npmrc` (do **not** commit this):

   ```
   //npm.pkg.github.com/:_authToken=YOUR_TOKEN
   ```

3. Publish (the `prepack` hook builds the library automatically):

   ```bash
   npm publish
   ```

## Consuming it in another project

1. Add an `.npmrc` in that project so the scope resolves to GitHub Packages, and
   authenticate with a token that has `read:packages`:

   ```
   @YOUR_LOGIN:registry=https://npm.pkg.github.com
   //npm.pkg.github.com/:_authToken=YOUR_READ_TOKEN
   ```

2. Install:

   ```bash
   npm install @YOUR_LOGIN/shardx-ui-kit
   ```

3. Wire up styles. **Pick the mode that matches the host project** — this matters:

### Mode A — host does NOT use Tailwind

Import the standalone, pre-compiled stylesheet once (no Tailwind, no SVGR needed):

```tsx
import '@YOUR_LOGIN/shardx-ui-kit/styles.css'
```

### Mode B — host ALSO uses Tailwind v4 (recommended for such hosts)

> ⚠️ Do **NOT** import `styles.css` here. It ships its own Tailwind layer and
> would collide with the host's Tailwind — breaking responsive/`lg:` precedence
> and layout. Instead, import only the **tokens** and let the host's single
> Tailwind generate the kit's utilities:

In the host's main Tailwind CSS file:

```css
@import "tailwindcss";
@import "@YOUR_LOGIN/shardx-ui-kit/tokens.css";
/* let Tailwind scan the kit's compiled classes (adjust the relative path
   so it points at node_modules from this CSS file): */
@source "../node_modules/@YOUR_LOGIN/shardx-ui-kit/dist";
```

Then use components normally:

```tsx
import { Button, ThemeProvider } from '@YOUR_LOGIN/shardx-ui-kit'
import { RiSearchLine } from '@YOUR_LOGIN/shardx-ui-kit/icons'

<ThemeProvider>
  <Button leftIcon={<RiSearchLine className="size-5" />}>Search</Button>
</ThemeProvider>
```

Overlays like `Modal` render into `#portal-root`; add `<div id="portal-root" />`
to the host `index.html`, or the kit falls back to `document.body`.

## Links & client-side navigation (Next.js, React Router…)

Kit components that render links (`Tabs`, `Breadcrumb`) use a native `<a>` by
default. To get framework routing benefits — Next.js prefetch, client-side
navigation and the router cache — inject your framework's link component once
via `LinkProvider`; every kit link then renders through it.

**Next.js (App Router)** — create a client provider and wrap your app:

```tsx
// app/providers.tsx
'use client'
import NextLink from 'next/link'
import { LinkProvider, ThemeProvider } from '@YOUR_LOGIN/shardx-ui-kit'

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <LinkProvider component={NextLink}>
      <ThemeProvider>{children}</ThemeProvider>
    </LinkProvider>
  )
}
```

```tsx
// app/layout.tsx
import { Providers } from './providers'

export default function RootLayout({ children }) {
  return (
    <html>
      <body><Providers>{children}</Providers></body>
    </html>
  )
}
```

**React Router**: `<LinkProvider component={RouterLink}>` (wrap its `to`/`href`
if needed). **No provider** → components fall back to `<a>` and just work.

## License

Kit: [MIT](./LICENSE). Icons: Remix Icon license — see
[src/icons/LICENSE](./src/icons/LICENSE).
