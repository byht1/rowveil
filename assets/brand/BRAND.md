# Roostbase Brand Pack

Ready-to-use assets for the Roostbase website, desktop app, PWA and social previews.

## Recommended usage

- **Main website header:** `logo/roostbase-horizontal-dark.svg` on a dark background or `roostbase-horizontal-light.svg` on a light background.
- **App icon:** `icons/app/roostbase-512.png` or `roostbase-1024.png`.
- **Favicon:** `icons/favicon/favicon.ico` plus 16/32/48 px PNGs.
- **PWA:** `icons/app/icon-192.png`, `icons/app/icon-512.png` and `web/manifest.webmanifest`.
- **Open Graph / social preview:** `social/roostbase-og-1200x630.png`.
- **Design tokens:** `web/roostbase-tokens.css` and `brand/colors.json`.

## Brand idea

The mark combines three ideas:
1. a **nest** = a safe, organized place for data;
2. a **database cylinder** = storage/schema;
3. a **terminal prompt** = querying and developer tooling.

## Colors

| Token | Hex |
|---|---|
| Primary | `#8B5CF6` |
| Primary light | `#A78BFA` |
| Primary dark | `#6D28D9` |
| Background | `#0B0B1A` |
| Surface | `#151529` |
| Accent | `#3B82F6` |
| Success | `#22C55E` |
| Warning | `#F59E0B` |
| Error | `#EF4444` |

## Typography

Use **Inter** where available. For native/desktop UI, the fallback stack in `roostbase-tokens.css` is recommended.

## Minimum logo guidance

- Prefer SVG for web UI and high-DPI desktop surfaces.
- Keep clear space around the logo roughly equal to the height of the terminal card inside the mark.
- Avoid recoloring individual internal elements independently; use the supplied full-color or monochrome versions.
- At very small sizes (16–32 px), use the symbol only, not the full wordmark.

## Included files

- `logo/` — symbol and wordmark variants in SVG + PNG.
- `icons/` — application, PWA and favicon sizes.
- `social/` — Open Graph/social card.
- `brand/` — brand board and machine-readable colors.
- `web/` — CSS tokens and PWA manifest.
