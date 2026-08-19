// ⌘K — one way in to everything in the model.
//
// This replaces a dropdown that was pinned under the sidebar's search box: it
// was 360px wide inside a 340px column with `overflow: hidden`, so the right
// edge of every result — the part that said what kind of thing it was — was
// cut off mid-word. An overlay has the whole window, so a result can say
// "ApplicationComponent" and a view can say how many elements it holds.

import { h, clear, fmt, relLabel } from "./dom.js";
import { store, search, elem, view, rel } from "./store.js";
import { typeIcon } from "./notation.js";
import { icon } from "./icons.js";
import { href } from "./router.js";

let root = null;
let hits = [];
let at = 0;
let lastFocus = null;

// Things to do that are not concepts. They are listed when the box is empty
// and matched by name when it is not, so ⌘K is also how you change page.
function commands() {
  return [
    { label: "Views", hint: "Go to", iconName: "view", run: () => (location.hash = href("views")) },
    { label: "Elements", hint: "Go to", iconName: "elements", run: () => (location.hash = href("elements")) },
    { label: "Relationships", hint: "Go to", iconName: "relations", run: () => (location.hash = href("relations")) },
    { label: "Graph", hint: "Go to", iconName: "graph", run: () => (location.hash = href("graph")) },
    { label: "Statistics", hint: "Go to", iconName: "stats", run: () => (location.hash = href("stats")) },
  ];
}

export function openPalette() {
  if (root) return;
  lastFocus = document.activeElement;

  const input = h("input", {
    type: "text", placeholder: "Search elements, relationships and views…",
    autocomplete: "off", spellcheck: "false", "aria-label": "Search the model",
    "aria-controls": "palette-list", "aria-autocomplete": "list", role: "combobox",
    "aria-expanded": "true",
  });
  const list = h("div", { class: "palette-list", id: "palette-list", role: "listbox" });
  const panel = h("div", { class: "palette", role: "dialog", "aria-modal": "true", "aria-label": "Search" },
    h("div", { class: "palette-field" }, icon("search"), input,
      h("kbd", null, "Esc")),
    list,
    h("div", { class: "palette-foot" },
      h("span", null, h("kbd", null, "↑↓"), " move"),
      h("span", null, h("kbd", null, "↵"), " open"),
      h("span", { class: "spacer" }),
      h("span", null, `${fmt(store.data.elements.length)} elements · ${fmt(store.data.relations.length)} relationships · ${fmt(store.data.views.length)} views`)));
  const scrim = h("div", { class: "overlay", onclick: closePalette });
  root = h("div", null, scrim, panel);
  document.body.appendChild(root);

  input.addEventListener("input", () => draw(input.value));
  input.addEventListener("keydown", onKey);
  draw("");
  input.focus();
}

export function closePalette() {
  if (!root) return;
  root.remove();
  root = null;
  hits = [];
  lastFocus?.focus?.();
}

export function paletteIsOpen() {
  return !!root;
}

function onKey(e) {
  if (e.key === "Escape") { e.preventDefault(); closePalette(); return; }
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    if (!hits.length) return;
    at = (at + (e.key === "ArrowDown" ? 1 : -1) + hits.length) % hits.length;
    mark();
  } else if (e.key === "Enter") {
    e.preventDefault();
    hits[at]?.run();
  }
}

function mark() {
  const rows = [...(root?.querySelectorAll(".palette-hit") || [])];
  rows.forEach((r, i) => {
    r.classList.toggle("is-active", i === at);
    r.setAttribute("aria-selected", String(i === at));
  });
  rows[at]?.scrollIntoView({ block: "nearest" });
}

function draw(q) {
  const list = root.querySelector(".palette-list");
  clear(list);
  hits = [];
  at = 0;

  const needle = q.trim().toLowerCase();
  const cmds = needle
    ? commands().filter((c) => c.label.toLowerCase().includes(needle))
    : commands();

  const group = (title, items) => {
    if (!items.length) return;
    list.appendChild(h("div", { class: "palette-group caps" }, title));
    for (const it of items) {
      const row = h("a", {
        class: "palette-hit", role: "option", href: it.href || "#",
        "aria-selected": "false",
        onclick: (e) => { e.preventDefault(); it.run(); },
        onmousemove: () => { at = hits.indexOf(it); mark(); },
      }, it.icon, h("span", { class: "ellipsis" }, it.label), h("span", { class: "hit-type" }, it.hint));
      hits.push(it);
      list.appendChild(row);
    }
  };

  if (needle) {
    const found = search(q, 8);
    group("Elements", found.elements.map(({ i }) => {
      const e = elem(i);
      return { label: e.name || "(unnamed)", hint: e.type, icon: typeIcon(e.type), href: href("element", e.id), run: () => go(href("element", e.id)) };
    }));
    group("Views", found.views.map(({ i }) => {
      const v = view(i);
      return { label: v.name, hint: `${fmt(v.elements.length)} elements`, icon: icon("view"), href: href("view", v.id), run: () => go(href("view", v.id)) };
    }));
    group("Relationships", found.relations.map(({ i }) => {
      const r = rel(i);
      const a = r.src >= 0 ? elem(r.src).name : r.srcId;
      const b = r.tgt >= 0 ? elem(r.tgt).name : r.tgtId;
      return { label: r.name || `${a} → ${b}`, hint: relLabel(r.type), icon: icon("relations"), href: href("relation", r.id), run: () => go(href("relation", r.id)) };
    }));
  }
  group(needle ? "Commands" : "Go to", cmds.map((c) => ({
    label: c.label, hint: c.hint, icon: icon(c.iconName), run: () => { closePalette(); c.run(); },
  })));

  if (!hits.length) {
    list.appendChild(h("div", { class: "empty" },
      h("p", { class: "empty-title" }, "No match"),
      h("p", { class: "empty-body" }, `Nothing in this model is called “${q.trim()}”.`)));
  }
  mark();
}

function go(hash) {
  closePalette();
  location.hash = hash;
}
