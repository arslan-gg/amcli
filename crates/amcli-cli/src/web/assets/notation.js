// Drawing ArchiMate the way the renderer does, in the browser: same fills,
// same outlines, same icons, same line ends. Everything here reads from the
// `types`, `relTypes` and `decos` tables the server put in the model blob, so
// there is one source of notation and the graph cannot drift from the views.

import { s } from "./dom.js";
import { store } from "./store.js";

export const ICON = 16;

// What a character costs, for deciding where a label breaks. The same 0.52 em
// the renderer charges, at the size the page draws — charging more than the
// renderer does made the page wrap a name the renderer fits on one line, and
// in a group's tab, which gets one line, that came out as half a name. Local:
// only `wrap` below has ever needed it.
const PER_CHAR = 11 * 0.52;

// The height of a group's tab, as `amcli-view`'s geometry has it.
export const GROUP_HEADER = 18;

// ColorFactory.getDerivedLineColor: the border Archi derives from a fill.
export function derivedLine(hex) {
  return scale(hex, 0.7);
}

// What the renderer paints a group's tab: the fill, a shade down.
export function darker(hex) {
  return scale(hex, 0.9);
}

function scale(hex, by) {
  const n = parseInt(hex.slice(1), 16);
  const f = (c) => Math.floor(c * by);
  const r = f((n >> 16) & 255), g = f((n >> 8) & 255), b = f(n & 255);
  return "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
}

export function typeOf(name) {
  return store.data.types[name] || { layer: "Other", figure: "rect", fill: "#ffffff", icon: null };
}

// The outline of a figure at (0,0) with the given size, as one SVG element.
export function figure(kind, w, h, attrs = {}) {
  switch (kind) {
    case "rounded":
      return s("rect", { width: w, height: h, rx: 10, ry: 10, ...attrs });
    case "circle": {
      const r = Math.min(w, h) / 2;
      return s("circle", { cx: w / 2, cy: h / 2, r, ...attrs });
    }
    case "octagon": {
      const I = 10;
      const pts = [[I, 0], [w - I, 0], [w, I], [w, h - I], [w - I, h], [I, h], [0, h - I], [0, I]];
      return s("polygon", { points: pts.map((p) => p.join(",")).join(" "), ...attrs });
    }
    case "tabbed": {
      // A tab across the top-left, then the body — Archi's GroupFigure, and
      // the renderer's: the tab is half the figure wide and a shade darker.
      const g = s("g", attrs);
      g.appendChild(s("rect", { ...attrs, width: Math.max(40, w / 2), height: GROUP_HEADER, fill: darker(attrs.fill || "#ffffff") }));
      g.appendChild(s("rect", { ...attrs, y: GROUP_HEADER, width: w, height: Math.max(1, h - GROUP_HEADER) }));
      return g;
    }
    default:
      return s("rect", { width: w, height: h, ...attrs });
  }
}

// One <symbol> per element type, once per SVG root; a figure then carries its
// icon as a <use>, exactly as a rendered view does. Line-end markers too.
export function ensureSymbols(svg) {
  if (svg.querySelector("defs[data-symbols]")) return;
  const defs = s("defs", { "data-symbols": "1" });
  for (const [name, t] of Object.entries(store.data.types)) {
    if (!t.icon) continue;
    const sym = s("symbol", { id: `i-${name}`, viewBox: `0 0 ${ICON} ${ICON}` });
    sym.appendChild(s("path", { d: t.icon, fill: "none", stroke: "currentColor", "stroke-width": 1, "stroke-linejoin": "round" }));
    defs.appendChild(sym);
  }
  // From the renderer's own templates: tip at the origin, body running back
  // along -x, which is exactly how an SVG marker is oriented.
  for (const [name, d] of Object.entries(store.data.decos)) {
    for (const end of ["end", "start"]) {
      const m = s("marker", {
        id: `m-${name}-${end}`,
        viewBox: "-24 -10 26 20", markerWidth: 26, markerHeight: 20,
        refX: 0, refY: 0, markerUnits: "userSpaceOnUse",
        orient: end === "start" ? "auto-start-reverse" : "auto",
      });
      const pts = d.points.map((p) => p.join(",")).join(" ");
      const solid = name.endsWith("filled");
      m.appendChild(s(d.filled ? "polygon" : "polyline", {
        points: pts,
        class: d.filled ? (solid ? "filled" : "hollow") : "",
        fill: d.filled ? null : "none",
      }));
      defs.appendChild(m);
    }
  }
  const ball = s("marker", { id: "m-ball-start", viewBox: "-5 -5 10 10", markerWidth: 10, markerHeight: 10, refX: 0, refY: 0, markerUnits: "userSpaceOnUse" });
  ball.appendChild(s("circle", { r: 3, class: "filled" }));
  defs.appendChild(ball);
  svg.insertBefore(defs, svg.firstChild);
}

// A small inline icon of a type, for tables and headings.
export function typeIcon(type, cls = "type-icon") {
  const t = typeOf(type);
  const svg = s("svg", { class: cls, viewBox: `0 0 ${ICON} ${ICON}` });
  if (cls.includes("boxed")) svg.style.background = t.fill;
  if (t.icon) svg.appendChild(s("path", { d: t.icon, fill: "none", stroke: "currentColor", "stroke-width": 1.2, "stroke-linejoin": "round" }));
  else svg.appendChild(s("circle", { cx: 8, cy: 8, r: 5, fill: "currentColor" }));
  return svg;
}

// Wrap a label into at most `maxLines` lines that fit `width` at ~6px/char.
export function wrap(text, width, maxLines = 3, perChar = PER_CHAR) {
  const max = Math.max(3, Math.floor(width / perChar));
  const lines = [];
  outer: for (const para of String(text).split("\n")) {
    let line = "";
    for (const word of para.split(/\s+/).filter(Boolean)) {
      if (!line) line = word;
      else if (line.length + 1 + word.length <= max) line += " " + word;
      else { lines.push(line); line = word; }
      if (lines.length >= maxLines) break outer;
    }
    if (line) lines.push(line);
    if (lines.length >= maxLines) break;
  }
  if (lines.length > maxLines) lines.length = maxLines;
  return lines.map((l) => (l.length > max ? l.slice(0, Math.max(1, max - 1)) + "…" : l));
}

// A graph node: figure, icon, wrapped label, positioned by its own transform.
export function nodeGroup(type, name, w, h) {
  const t = typeOf(type);
  const line = t.figure === "tabbed" ? "#5c5c5c" : derivedLine(t.fill);
  const g = s("g", { class: "node" });
  g.appendChild(figure(t.figure, w, h, { class: "figure", fill: t.fill, stroke: line }));
  const showIcon = t.icon && !["tabbed", "circle"].includes(t.figure) && w >= 40 && h >= 26;
  if (showIcon) g.appendChild(s("use", { href: `#i-${type}`, x: w - 20, y: 4, width: ICON, height: ICON, color: line }));
  if (t.figure !== "circle") {
    // A group's name goes in its tab, where the renderer puts it, and gets
    // the one line the tab is tall — centred over the whole figure it fell
    // across the seam between the tab and the body and read as clipped.
    // Every other figure centres its label in the box.
    // A group's name goes in its body, not in its tab: the tab is half the
    // figure wide and one line tall, so any real name either ran out through
    // its right edge or had to be cut to one word. The body is the whole
    // width, and the layout makes a group a tab taller for exactly this.
    const tabbed = t.figure === "tabbed";
    const inset = tabbed ? 5 : showIcon ? 22 : 6;
    const lh = 13;
    const lines = wrap(name || "", w - 2 * inset, 3);
    const band = tabbed ? h - GROUP_HEADER : h;
    const mid = (tabbed ? GROUP_HEADER : 0) + band / 2;
    const top = mid - ((lines.length - 1) * lh) / 2;
    lines.forEach((l, i) => {
      g.appendChild(s("text", { x: w / 2, y: top + i * lh, "text-anchor": "middle", "dominant-baseline": "middle" }, l));
    });
  }
  return g;
}

// Line style of a relationship, adjusted for the two types whose look depends
// on an attribute: Access points the way the data moves, Association gets a
// half arrow only when directed. accessType 0 is Write, 1 Read, 2 Unspecified,
// 3 Read/Write — the obvious guess is wrong, see the model layer.
export function relStyle(r) {
  const base = store.data.relTypes[r.type] || { dash: null, source: "none", target: "none" };
  let { dash, source, target } = base;
  if (r.type === "Access") {
    const a = r.access === null || r.access === undefined ? 0 : r.access;
    source = a === 1 || a === 3 ? "arrow-open" : "none";
    target = a === 0 || a === 3 ? "arrow-open" : "none";
  } else if (r.type === "Association") {
    target = r.directed ? "half-arrow" : "none";
  }
  return { dash, source, target };
}

export function accessLabel(a) {
  return { 0: "write", 1: "read", 2: "unspecified", 3: "read/write" }[a] ?? "";
}
