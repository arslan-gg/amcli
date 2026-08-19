// The interface's own icons — one 16×16 box, one 1.5 stroke, one set.
//
// These are *not* the ArchiMate type icons. Those live in
// `crates/amcli-view/src/icons.rs`, are ported from Archi's figure classes and
// arrive through `/api/model`; `notation.js` draws them. What the chrome used
// instead was fifteen unicode characters — ‹ ▣ ▶ ▼ ✕ ↗ ↓ ⤡ ◐ → ← ↔ − ▾ + —
// each at the surrounding font's size, on its own baseline, in whatever face
// the platform happened to have for it, sitting on the same line as a drawn
// figure. So the domain notation was systematic and the chrome was not.
//
// Everything here is stroked, so an icon takes the colour of the text beside
// it and needs no light and dark variant.

import { s } from "./dom.js";

// Path data only. Anything filled says so with a leading "f:".
const PATHS = {
  // chevrons and arrows
  "chevron-right": "M6 3.5 10.5 8 6 12.5",
  "chevron-left": "M10 3.5 5.5 8 10 12.5",
  "chevron-down": "M3.5 6 8 10.5 12.5 6",
  "chevron-up": "M3.5 10 8 5.5 12.5 10",
  "arrow-right": "M2.5 8h11M9.5 4 13.5 8l-4 4",
  "arrow-left": "M13.5 8h-11M6.5 4 2.5 8l4 4",
  "arrow-both": "M2.5 8h11M5.5 5 2.5 8l3 3M10.5 5l3 3-3 3",
  "arrow-down": "M8 2.5v9M4.5 8 8 11.5 11.5 8",
  "sort-asc": "M8 12.5v-9M4.5 7 8 3.5 11.5 7",
  "sort-desc": "M8 3.5v9M4.5 9 8 12.5 11.5 9",

  // actions
  close: "M4 4l8 8M12 4l-8 8",
  plus: "M8 3.5v9M3.5 8h9",
  minus: "M3.5 8h9",
  check: "M3.5 8.5 6.5 11.5 12.5 4.5",
  search: "M10.9 10.9 13.5 13.5M10.75 7a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z",
  download: "M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10",
  external: "M12.5 9.5v3h-9v-9h3M9.5 3.5h3v3M12.5 3.5 7.25 8.75",
  expand: "M9.5 6.5 13.5 2.5M10 2.5h3.5V6M6.5 9.5 2.5 13.5M6 13.5H2.5V10",
  collapse: "M13 3 9.5 6.5M9.5 3.5v3h3M3 13 6.5 9.5M6.5 12.5v-3h-3",
  fit: "M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10",
  theme: "M13.5 8a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0Z|f:M8 2.5v11A5.5 5.5 0 0 0 8 2.5Z",
  filter: "M2.5 3.5h11L9.5 8.5v4.5l-3-1.75V8.5Z",
  pin: "M6 2.5h4l-.5 4 2 2v1.25H4.5V8.5l2-2ZM8 9.75v3.75",
  copy: "M5.5 5.5h8v8h-8ZM10.5 5.5v-3h-8v8h3",
  more: "f:M4.25 8a1.25 1.25 0 1 1-2.5 0 1.25 1.25 0 0 1 2.5 0Z|f:M9.25 8a1.25 1.25 0 1 1-2.5 0 1.25 1.25 0 0 1 2.5 0Z|f:M14.25 8a1.25 1.25 0 1 1-2.5 0 1.25 1.25 0 0 1 2.5 0Z",

  // places
  view: "M2.25 3.25h5v3.5h-5ZM8.75 9.25h5v3.5h-5ZM4.75 6.75v2.75a1 1 0 0 0 1 1h3",
  elements: "M2.5 2.5h5v5h-5ZM8.5 2.5h5v5h-5ZM2.5 8.5h5v5h-5ZM8.5 8.5h5v5h-5Z",
  relations: "M5.5 4a1.75 1.75 0 1 1-3.5 0 1.75 1.75 0 0 1 3.5 0ZM14 12a1.75 1.75 0 1 1-3.5 0 1.75 1.75 0 0 1 3.5 0ZM4.75 5.5C4.75 10 7 11.25 10.5 11.75",
  graph: "M6 3.5a1.75 1.75 0 1 1-3.5 0 1.75 1.75 0 0 1 3.5 0ZM13.5 6a1.75 1.75 0 1 1-3.5 0 1.75 1.75 0 0 1 3.5 0ZM10 12.5a1.75 1.75 0 1 1-3.5 0 1.75 1.75 0 0 1 3.5 0ZM5.75 4.5 10.2 5.6M11.3 7.6 9.1 10.9M7 5.1 8 10.8",
  folder: "M2.5 12.5v-8a1 1 0 0 1 1-1h3l1.5 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9.5a1 1 0 0 1-1-1Z",
  doc: "M4 3.5h8M4 6.5h8M4 9.5h5",

  // shell
  rail: "M2.5 4a1.5 1.5 0 0 1 1.5-1.5h8A1.5 1.5 0 0 1 13.5 4v8a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 12ZM6.5 2.5v11",
  inspector: "M2.5 4a1.5 1.5 0 0 1 1.5-1.5h8A1.5 1.5 0 0 1 13.5 4v8a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 12ZM9.5 2.5v11",

  // states
  dot: "f:M11 8a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
  alert: "M8 2.5 14 13H2ZM8 6.5v3M8 11.25v.5",
  info: "M13.5 8a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0ZM8 7.5v3.5M8 5.25v.5",
};

// One icon. `size` is only ever the token's 16 unless a caller is drawing a
// larger boxed figure, which the type icons — not these — do.
export function icon(name, opts = {}) {
  const data = PATHS[name];
  const svg = s("svg", {
    class: "icon" + (opts.class ? ` ${opts.class}` : ""),
    viewBox: "0 0 16 16",
    width: opts.size || 16,
    height: opts.size || 16,
    fill: "none",
    stroke: "currentColor",
    "stroke-width": opts.weight || 1.5,
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
    "aria-hidden": "true",
    focusable: "false",
  });
  if (!data) return svg; // an unknown name draws nothing rather than throwing
  for (const part of data.split("|")) {
    const filled = part.startsWith("f:");
    svg.appendChild(s("path", {
      d: filled ? part.slice(2) : part,
      fill: filled ? "currentColor" : "none",
      stroke: filled ? "none" : "currentColor",
    }));
  }
  return svg;
}
