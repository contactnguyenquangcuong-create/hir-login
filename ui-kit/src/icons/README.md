# Icons

SVG icons imported as React components via [SVGR](https://react-svgr.com/)
(`vite-plugin-svgr`). Every icon keeps `fill="currentColor"`, so its colour
follows the surrounding text colour — and therefore the active light/dark theme.

Source: the **Remix Icon** set (the same library used on the Figma Icons page),
vendored under [`svg/`](./svg). See [LICENSE](./LICENSE).

## Usage

Named component (recommended):

```tsx
import { RiSearchLine, RiArrowRightSLine } from '@/icons'

<RiSearchLine className="size-5 text-text-sub-600" />
<RiArrowRightSLine className="size-4" />   // inherits currentColor
```

Direct file import (any icon in `svg/`):

```tsx
import SearchLine from '@/icons/svg/search-line.svg?react'

<SearchLine className="size-5" />
```

Icons default to `1em` so `className` sizing (`size-4`, `size-5`, …) and text
`color` control size and colour. There are 3,229 icons — names follow the Remix
convention in PascalCase with an `Ri` prefix (e.g. `close-line` → `RiCloseLine`,
`user-3-fill` → `RiUser3Fill`).
