# OpenForensic Imager Design Direction

Status: agent-proposed baseline for implementation. The examiner team should
replace or approve these choices before a release candidate.

## Identity

OpenForensic Imager is an examination bench, not a dashboard. The interface
should feel like a precise instrument: dense enough for long directory lists,
quiet enough for sustained reading, and explicit about evidence state.

## Dials

- ENERGY 1: calm, linear, restrained.
- RHYTHM 2: mostly consistent work surfaces with deliberate section breaks
  between navigation, inspection, and evidence properties.
- MOTION 1: no decorative animation; only immediate state changes.

## Palette

The palette uses two neutral families and one accent. Semantic red and green
are reserved for textual error and success labels and always appear with text.

| Token | Light | Dark | Purpose |
|---|---|---|---|
| Background | `#F4F1EA` | `#141311` | Work surface |
| Surface | `#FBFAF6` | `#1D1B18` | Panels and rows |
| Raised | `#FFFFFF` | `#26231F` | Menus and focused rows |
| Text | `#1C1B18` | `#F3EEE4` | Primary content |
| Muted | `#6F6A5E` | `#A8A294` | Secondary labels |
| Border | `#8A8578` | `#6F6A5E` | Interactive boundaries |
| Accent | `#8A4B00` | `#E0A458` | Focus, selection, primary action |
| Error | `#A33A2A` | `#FF9B8A` | Error text and error badge |
| Success | `#2F5D3A` | `#9FD3A8` | Success text and success badge |

All text pairings above were checked against WCAG AA with the antislop contrast
checker. The accent is used for focus rings, active selection, and the primary
action only.

## Typography

- UI: Noto Sans. It is installed on the host, covers the fixture names, and
  keeps dense labels readable.
- Technical values: Noto Sans Mono for offsets, hashes, and hexadecimal rows.
- No monospace headings and no uppercase tracking as decoration.

## Layout

- Native menu bar and toolbar for discoverability and keyboard shortcuts.
- Dockable evidence tree on the left, file list in the center, properties on the
  right, and viewer plus tasks at the bottom.
- Every panel has an empty state that names the next action.
- Evidence content is rendered as plain text; no HTML from evidence is ever
  interpreted.

## Interaction

- Row selection is visible with a border and accent marker, not color alone.
- Status labels always include text: Allocated, Deleted, Succeeded, Partial,
  Failed, Cancelled.
- Focus is always visible and uses the accent color at 3:1 or better against
  adjacent surfaces.
- No fake evidence, fake task rows, fake metrics, or decorative status dots.
