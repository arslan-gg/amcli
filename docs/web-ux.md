# The viewer's design system

`amcli web` worked. What it lacked was a system: every page invented its own
header, its own way to filter, its own way to say "seven of two hundred". Six
pages invented six of each, and the result read as *almost* consistent — which
is worse to look at than a page that never tried, because the eye keeps
finding near-misses.

This is the record of why, and of what was done about it. The diagnosis is
kept in full because it is the reason the constraints in
[CLAUDE.md](../CLAUDE.md) exist: each one is a test now, and a test whose
reason has been forgotten is a test somebody deletes.

Everything measured here was measured on a 145-element, 372-relationship,
24-view model at 1280×800, in both themes.

## What was already right, and had to survive

Start here, because a change that breaks any of these is a regression however
good it looks. All of them still hold.

- **No build step, no bundler, no CDN.** Plain ES modules and one stylesheet,
  compiled in with `include_str!`. The binary is the whole product and it works
  offline. Nothing below adds a dependency.
- **One notation.** `notation.js` draws from the `types`, `relTypes` and
  `decos` tables the server puts in `/api/model`, so the graph cannot drift
  from a rendered view. This was the one part of the UI that already *was* a
  design system; the rest was built to match it, not to replace it.
- **One layout engine.** `/api/layout` runs the same `layout::place` that
  `view auto` runs. The page decides what to draw; the server decides where.
- **Read-only by construction.** GET is the only verb.
- **`replaceParams` does not notify the router.** A page that changes its own
  filter redraws the part that changed. Keep it.
- **The URL is printed before the server serves**, and server threads never
  print.

## Why it looked inconsistent

Not a taste problem and not a discipline problem. Eight structural causes —
each of which produces inconsistency mechanically, whoever writes the code.

### 1. There is no component layer, so every page built its own

The sortable table header, `function th(label, k, cls)`, was written three
times — in `pages/views.js`, `pages/elements.js` and `pages/relations.js` —
with near-identical bodies and three different defaults for which columns sort
descending first. The filter→sort→cap→render pipeline is written three times.
`const push = () => replaceParams({…})` is written four times with four
different parameter sets.

`const LAYERS = [...]` is declared three times. `LAYER_LABEL` is declared three
times **with different contents**: `graph.js` abbreviates
`"Implementation & Migration"` to `"Impl. & Migration"`, `elements.js` and
`stats.js` do not. The same layer has two names in the same application.

When one widget is written three times it drifts three ways. There is nothing
to be consistent *with*.

### 2. Tokens exist for colour and for nothing else

`:root` carries a clean set of semantic colour roles — `--background`,
`--foreground`, `--muted`, `--muted-foreground`, `--card`, `--border`,
`--input`, `--ring`, `--primary`, `--accent`. That vocabulary is shadcn/Radix's,
and taking it was the right call.

Then the system stops. `app.css` contains **nine distinct font sizes**
(9, 10, 11, 12, 13, 14, 15, 18, 22px), roughly twenty distinct spacing values,
and four radius idioms (`var(--radius)`, `4px`, `2px`, `999px`). On top of that
the page modules carry **37 inline `style: {}` objects** that duplicate and
contradict the stylesheet:

| Property | Values in use |
|---|---|
| search input width | `220px`, `240px`, `100%` |
| `marginTop` | `6px`, `10px`, `12px`, `24px` |
| `maxWidth` | `220px`, `320px`, `420px`, `600px`, `820px` |

A token system with holes produces exactly the "nearly aligned" look that
prompted this document.

### 3. `.page-head` is a flex bag, not a slotted component

`display: flex; flex-wrap: wrap` and each page throws in whatever it has. The
consequences at 1280px:

- Only **Stats** has a page title. Views, Elements, Relations and Graph have
  none; the detail page's title is the word "Element".
- Elements wraps to two rows, Graph to two rows plus a separate filter grid,
  View to two rows with **`PNG ↓` orphaned alone on the second line**.
- Because it *wraps* rather than reflows, the header's shape is a function of
  content width. A model with more layers changes the layout of the page.

On the graph, header (95px) plus filter bar (115px) is **210px — 26% of the
window height** spent on controls before the drawing starts. With the 340px
sidebar, the canvas gets 54% of the screen.

### 4. Four filtering idioms for one job

| Page | How you filter |
|---|---|
| Views | folder **tree** + name box |
| Elements | layer **chips** + type `<select>` + folder `<select>` + name box |
| Relations | type **chips** + name box |
| Graph | labelled rows of **popover checkbox menus** + pins, in a separate bar |

Worse, the same dimension is offered two ways. Folder is a scrollable tree on
Views and a flat 41-item `<select>` on Elements. Type is a `<select>` on
Elements and a checkbox popover on Graph. A reader who learns one page has
learned nothing about the next.

### 5. Selection has two destinations and no single "current thing"

Clicking a table row opens the sidebar panel. Clicking the name *inside* that
row navigates to a full page. Same row, two outcomes, nothing in the visuals to
tell them apart.

Then `↗ Maximize` / `⤡ Minimize` shuttles between two renderings of the same
content, with `maximizedFrom` remembering exactly one step. Arrive at
`#/element/ID` from search or a pasted URL and Minimize sends you to
`#/views` — somewhere you have never been. There is no breadcrumb, and the
browser's own Back is not part of the design.

### 6. The chrome's icons are text

Fifteen unicode characters stand in for an icon set: `‹ ▣ ▶ ▼ ✕ ↗ ↓ ⤡ ◐ → ← ↔ − ▾ +`.
They inherit the surrounding font size, sit on per-glyph baselines and render in
whatever font the platform has for them. They appear on the same line as the
real 16×16 SVG type icons from `icons.rs`. A view was `"▣ "` in four separate
places while the element beside it got a drawn figure.

So the domain notation is systematic and the chrome is not, and they share a row.

### 7. Space is allocated by convention, not by task

`--sidebar-w: 340px` is fixed — 27% of a 1280px window — and holds a wordmark, a
search box and five links. There is **one `@media` query in the entire
stylesheet**, and it is for the stats grid. Nothing collapses, nothing resizes.

Meanwhile the densest content in the application, the details panel, is jammed
into the bottom of that same column: **339×386px displaying 816px of content**.
Under half of a concept is visible at once, on a screen with 940px of unused
width beside it.

### 8. Keyboard and focus were never designed

- **No `:focus-visible` rule anywhere.** Only the two text inputs have a
  designed ring.
- Sortable `<th>`s and tree rows are click-only elements with no `tabindex`, no
  `role` and no key handler. Sorting a table and picking a folder are
  unreachable from the keyboard (WCAG 2.1.1). Only the links inside cells are.
- No `aria-sort` on a sortable column, no `aria-pressed` on a toggle chip.
- Search is a custom listbox with no `role="combobox"`, `aria-expanded` or
  `aria-activedescendant`.
- Two `aria-label`s and zero `role`s in the whole application.
- No `prefers-reduced-motion`.

## Defects found while measuring

These are bugs, not preferences. Fix them regardless of what happens to the
rest of the plan.

| | Where | What |
|---|---|---|
| 1 | `.search-results` | Fixed `width: 360px` inside a 340px sidebar with `overflow: hidden`. The panel is **clipped by 32px** and every result's type label is cut mid-word. |
| 2 | `.chip.active` | The count is an inner `<span class="muted">`, which wins over the chip's inverted colour. **3.67:1 in light, 2.46:1 in dark** — both fail WCAG AA. Present on Elements and Relations. |
| 3 | `kbd` | `--muted-foreground` on `--muted` = **4.40:1**, fails AA. |
| 4 | `renderLegend()` | Only called from `draw()`, so on an empty graph the legend renders as an **empty 32×18 bordered box** in the corner. |
| 5 | `.dim { opacity: .18 }` | Selecting a node fades everything else to near-invisible; the graph reads as broken rather than focused. |
| 6 | `app.css:389` | Dead rule: `.canvas .edge.selected + * {}`. |
| 7 | Terminology | Nav says **Relations**, the column header says **Relationships**, the detail page says **Relationship**, and the graph's *Relations* row counts **kinds**. |
| 8 | Relations table | Default sort is by type, so the first column is one word repeated down the screen; **Name is empty for every row** in a typical model; **Detail** is a vague header for a mostly-empty column. |
| 9 | Stats cards | `auto-fill minmax(150px, 1fr)` yields five cards then one orphan, and the wrapping label "orphans — connected to nothing" makes its card taller than its row. |
| 10 | Stats bars | Two side-by-side panels scale to independent maxima, inviting a comparison the lengths do not support. |
| 11 | Detail page | An 820px single column inside a 940px pane. "Maximize" buys line length and nothing else. |
| 12 | Table rows | Row height varies with folder-name wrap, breaking the scan rhythm. |

## The system

**Finish the choice the stylesheet already started, and pair it with the
interaction archetype the application actually is.**

Three layers, all hand-written, no dependency, no build step.

### Layer 1 — Tokens

Extend the shadcn/Radix semantic-role vocabulary already in `:root` from colour
alone to the full set: **space, type, radius, elevation, density, motion,
focus**. One 4px base for space; a five-step type scale; three radii; two
elevations; one focus ring. Every value used by the UI comes from a token, and
`assets/*.js` stops carrying literal pixels.

### Layer 2 — Primitives

A closed set, each written once, in `ui.js` plus its CSS:

`AppShell` · `Rail` · `Toolbar` · `FilterBar` · `Menu` · `DataTable` · `Tree` ·
`Inspector` · `Canvas` (HUD, legend, empty state) · `Button` · `Chip` ·
`Badge` · `Card` · `StatBar` · `EmptyState` · `CommandPalette`

`DataTable` owns sorting, sticky headers, selection, the cap note, `aria-sort`
and arrow-key navigation — once, for all three tables.

### Layer 3 — Patterns

The fixed answer to each recurring question, so a page never has to decide:

- **Every page header has the same anatomy**: title · scope · controls ·
  spacer · result count. Never wraps; it scrolls or collapses into a menu.
- **One filter idiom**: a `FilterBar` of labelled dimensions, each a popover
  with counts, plus a chip per active exclusion. Hierarchical dimensions
  (folder) always get a tree, in the popover, on every page.
- **One count sentence**: `N of M`, in the same place, in the same words.
- **One selection**: clicking anything anywhere sets the current concept and
  fills the Inspector. Nothing navigates on a single click.
- **One place for detail**: the Inspector. `#/element/ID` is a deep link that
  opens it, not a second rendering of it.
- **One icon system**: 16×16 SVG, from `notation.js`, for domain *and* chrome.

### The interaction archetype

An **inspector workbench** — VS Code, Linear, a database console — not a
document site. Three panes:

```
┌────────┬──────────────────────────────┬───────────────┐
│ Rail   │ Working surface              │ Inspector     │
│ 208px  │ table / tree+table / canvas  │ 320–560px     │
│ ↔ 48px │                              │ resizable     │
│        │                              │ opens on      │
│ nav    │                              │ selection     │
│ only   │                              │               │
└────────┴──────────────────────────────┴───────────────┘
```

This single move fixes cause 7: the rail carries navigation and nothing else at
208px collapsible to a 48px icon rail; the Inspector — the densest content —
gets a resizable 320–560px on the right where it can be read beside the thing
it describes.

## What shipped

All seven phases are in. The viewer is rebuilt on the system above; what
follows is what changed and where it lives.

### The guardrails came first

Four tests in `crates/amcli-cli/tests/cli.rs`, written before a pixel moved:

| Test | Fails on |
|---|---|
| `tokens_are_the_only_literals` | a hex, an `rgb()` or any `px` but `0`/`1` in `app.css` |
| `page_modules_decide_no_lengths` | a quoted `12px` in any page module |
| `the_chrome_has_no_text_icons` | `▣ ▶ ▼ ✕ ↗ ⤡ ◐ ‹ ↔ ▾` anywhere but the comment that retired them |
| `every_token_pair_clears_wcag_aa` | any foreground/ground pair under 4.5:1, in either theme |

They are grep-shaped reads over the embedded assets, plus one contrast
calculation. No browser, no runtime cost, and they run with `cargo test`.

### The files

| File | What it is |
|---|---|
| `assets/tokens.css` | Every colour and every length in the interface, named. The only file allowed to hold one. |
| `assets/app.css` | Primitives and patterns, built from tokens alone. |
| `assets/ui.js` | The primitives: `toolbar`, `filterBar`, `dataTable`, `tree`, `popover`, `anchorTo`, `button`, `chip`, `badge`, `card`, `barChart`, `emptyState`, `kv`. Knows nothing about ArchiMate. |
| `assets/icons.js` | The chrome's icon set — one 16×16 box, one 1.5 stroke, `currentColor`. Not the ArchiMate type icons, which stay in `amcli-view/src/icons.rs`. |
| `assets/palette.js` | ⌘K, over the whole window. |
| `assets/pages/collection.js` | Views, Elements and Relationships — one page, three specs. |
| `assets/pages/view.js`, `graph.js`, `stats.js`, `detail.js` | The drawing, the graph, the numbers, the inspector. |

`pages/views.js`, `pages/elements.js` and `pages/relations.js` are gone, and
with them three `th()`s, three `rows()`s, three `renderTable()`s, three
`LAYERS` and three `LAYER_LABEL`s.

### The shell

Three columns that are always all three — rail, working surface, inspector.
Neither side pane opens or closes; each only narrows, so the layout never
jumps and nothing has to be found again. The inspector's width is dragged and
remembered.

**The rail is where you narrow; the middle is what you got.** Navigation, the
folder tree and the filters all live in the rail, on every page, through
`railContext()`. The horizontal filter band is gone, and so is the split pane
that used to hold the tree — the table has the middle to itself.

Measured at 1280×800 on the graph, against the same model as the audit:

| | Before | After |
|---|---|---|
| Chrome above the canvas | 210px | 42px |
| Canvas share of the window | 54% | 78% |
| Details panel | 339×386, fixed, under the menu | 340px wide, resizable, full height |

### Everything else, against the eight causes

1. **No component layer** → `ui.js`. `dataTable` owns sorting, sticky heads,
   selection, the cap note, `aria-sort` and arrow keys, once.
2. **Tokens for colour only** → six type steps, one 4px space scale, three
   radii, two elevations, two control heights, one focus ring, all named.
3. **`.page-head` a flex bag** → one `toolbar`, which neither wraps nor
   scrolls sideways: what does not fit moves into an overflow menu.
4. **Four filter idioms** → one `filterBar`, and a folder is always a tree.
   Clicking a folder opens it and shows what is in it in the same click.
5. **Two destinations per row** → one. A single click anywhere selects and
   fills the inspector; nothing navigates on a single click. Maximize and
   minimize are gone; `#/element/ID` opens the collection with it selected.
6. **Text as icons** → `icons.js`, and a test.
7. **Space by convention** → see the table above.
8. **No keyboard, no focus** → one `:focus-visible` for everything, arrow keys
   in tables and trees, `aria-sort`, `role="tree"`/`treeitem`, `⌘K` `⌘B` `⌘I`
   `/` `Esc`, and `prefers-reduced-motion`.

All twelve defects are fixed. The search panel is now an overlay with the
whole window, which fixed the clipping by deleting the thing that was broken;
chip counts use `--invert-subtle`; the legend only draws when there is a
legend to draw; `.is-dim` recedes at 0.35 instead of vanishing at 0.18; the
vocabulary comes from one place, so it is *Relationships* everywhere;
Relationships opens sorted by source rather than repeating one word down the
first column; and the stats cards fill their row.

One thing was cut rather than kept: the graph's layout picker. `layered` and
`auto` differ only when the layering comes out wide enough for the grid to be
worth taking, and with the graph's lanes free it almost never does — so the
menu offered the same drawing twice and a grid nobody wanted. The graph asks
for `auto`, which is what `view auto` runs.

## The consistency contract

Six rules. If a change breaks one, the change is wrong — and four of them fail
in `cargo test` rather than in review.

1. **No literal in a page module.** Colour, space, size, radius and duration
   come from tokens.
2. **No widget written twice.** A second copy is a primitive that has not been
   extracted yet.
3. **One page header anatomy**, and it never wraps.
4. **One filter idiom**, whatever the dimension.
5. **One name per concept**, from the glossary.
6. **Every interactive element is reachable and visibly focusable from the
   keyboard.**
