# Phase 5 Desktop UI Verification Results

Date: 2026-10-30

## Implemented scope

- C++ `QMainWindow` shell with native menu bar, toolbar, and status bar.
- QML workbench with evidence tree, file list, properties, viewer tabs, and
  background task panel in resizable split views.
- Evidence tree flow: open image, identify format read-only, analyze
  partitions, open filesystem at a partition offset, list root directory.
- File list flow: select file, inspect metadata, load bounded hex/text preview
  and signature through background jobs.
- Task panel with live progress, cancel, and failure/partial/cancelled labels.
- Light and dark theme tokens with verified WCAG AA text pairings.
- Empty, loading, error, partial, and cancelled states designed for each data
  view.
- Keyboard shortcuts: `Ctrl+O` add image, `Ctrl+Shift+T` toggle theme, and
  standard Quit.
- Evidence-controlled text rendered as plain text only.

## Design direction

`DESIGN.md` records the direction: ENERGY 1, RHYTHM 2, MOTION 1, Noto Sans plus
Noto Sans Mono, two neutral families plus one amber accent. No fabricated
evidence, metrics, task rows, or status dots appear in the UI.

## Verification evidence

```text
cmake -S . -B build/root -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build/root
ctest --test-dir build/root --output-on-failure
/usr/lib/qt6/bin/qmllint -I ui/qml ui/qml/*.qml
```

Result: 4 CTest tests passed:

| Test | Result |
|---|---|
| `openforensic-imager-tests` (11 model tests) | Passed |
| `openforensic-imager-self-test` light theme | Passed |
| `openforensic-imager-self-test-dark` | Passed |
| `openforensic-imager-qml-tests` (4 QML tests) | Passed |

`qmllint` exits 0 with no warnings. Both themes were rendered offscreen and
inspected; screenshots are not committed.

The Rust workspace remains at 39 executables and 118 passing tests after the
ABI additions used by the shell.

## Delivery Gate

- Hard Gate: no em dash, overflow, fabricated statistics or testimonials, dead
  links, low-contrast text, non-functional controls, missing states, keyboard
  traps, external CSS patches, broken theme mode, or unmeasured performance
  claims.
- Purpose Gate: the amber accent marks selection, focus, and primary actions
  only; no gradients, glow, glass, stock illustration, or decorative pattern.
- Liveliness: dials declared; one focal point per panel; structural spacing;
  one accent; identity from instrument-like density and evidence provenance.
- Craftsmanship: every control has behavior, evidence strings are escaped as
  plain text, and empty/loading/error states are present.

## Limitations

- The UI is not yet packaged. DEB/AppImage work remains Phase 7.
- The analysis worker process and sandbox are not implemented; parsing still
  runs in-process through the Rust library. Phase 6 must isolate it.
- Image and PDF preview rendering remain unimplemented. The UI shows signature
  and metadata only.
- Directory paging is not yet exposed by the filesystem layer, so very large
  directories still materialize before display.
- The evidence manager, acquisition wizard, verification report view, and
  extraction report view are not yet wired into the desktop shell.
- Accessibility was checked structurally (labels, focusable lists, keyboard
  shortcuts, contrast) but not with a screen reader or automated accessibility
  tree test.
