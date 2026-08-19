// The shell: rail, working surface, inspector — plus the routing, the theme,
// the keyboard and the poll that keeps all three in step with the file.
//
// The shell owns *where* things go. What goes in the middle is a page module,
// and what goes in the inspector is always `renderConcept`, whoever selected
// it. There is one current concept and one place it is shown.

import { h, clear, fmt } from "./dom.js";
import { store, load, subscribe, startPolling } from "./store.js";
import { parse, onRoute, href } from "./router.js";
import { icon } from "./icons.js";
import { iconButton } from "./ui.js";
import { openPalette, closePalette, paletteIsOpen } from "./palette.js";
import { renderConcept } from "./pages/detail.js";
import * as collection from "./pages/collection.js";
import * as viewPage from "./pages/view.js";
import * as graph from "./pages/graph.js";
import * as stats from "./pages/stats.js";

const NAV = [
  { page: "views", label: "Views", iconName: "view", count: (d) => d.views.length },
  { page: "elements", label: "Elements", iconName: "elements", count: (d) => d.elements.length },
  { page: "relations", label: "Relationships", iconName: "relations", count: (d) => d.relations.length },
  { page: "graph", label: "Graph", iconName: "graph" },
  { page: "stats", label: "Statistics", iconName: "stats" },
];

// Which nav entry a route lights up. A concept's deep link belongs to the
// collection it came from, because that is what the page behind it shows.
const OWNER = { view: "views", element: "elements", relation: "relations" };

const app = document.getElementById("app");
const main = document.getElementById("main");
const inspector = document.getElementById("inspector");
const inspectorBody = document.getElementById("inspector-body");
const inspectorActions = document.getElementById("inspector-actions");
const statusEl = document.getElementById("status");
const railContextEl = document.getElementById("rail-context");

// Where a page puts what narrows it — the folder tree, the filters, the pins.
// One place, on every page, so "how do I see less of this" has one answer.
export function railContext() { return railContextEl; }

let unmount = () => {};
let currentId = null;

/* ---- preferences ----------------------------------------------------------
   Storage can be denied outright; the viewer then simply keeps its defaults
   for the visit rather than failing to start. */
const prefs = {
  get(k, fallback) { try { return localStorage.getItem(k) ?? fallback; } catch { return fallback; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* not persisted */ } },
};

/* ---- theme ---------------------------------------------------------------- */
function applyTheme(t) {
  document.documentElement.dataset.theme = t;
  prefs.set("amcli-theme", t);
  themeBtn?.replaceChildren(icon(t === "dark" ? "theme" : "theme"));
  themeBtn?.setAttribute("title", t === "dark" ? "Switch to the light theme" : "Switch to the dark theme");
}
const themeBtn = iconButton("theme", "Switch theme", () =>
  applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark"), { variant: "quiet" });
applyTheme(prefs.get("amcli-theme") || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"));

/* ---- rail ------------------------------------------------------------------ */
const railToggle = document.getElementById("rail-toggle");
function applyRail(collapsed) {
  app.classList.toggle("rail-collapsed", collapsed);
  prefs.set("amcli-rail", collapsed ? "1" : "0");
  clear(railToggle).appendChild(icon("rail"));
  railToggle.title = collapsed ? "Expand the sidebar (⌘B)" : "Collapse the sidebar (⌘B)";
  railToggle.setAttribute("aria-label", railToggle.title);
}
railToggle.addEventListener("click", () => applyRail(!app.classList.contains("rail-collapsed")));
applyRail(prefs.get("amcli-rail") === "1");

document.getElementById("open-palette").addEventListener("click", openPalette);

const nav = document.getElementById("nav");
function buildNav() {
  clear(nav);
  for (const n of NAV) {
    nav.appendChild(h("a", { href: href(n.page), dataset: { page: n.page }, title: n.label },
      icon(n.iconName),
      h("span", { class: "nav-label" }, n.label),
      n.count ? h("span", { class: "nav-n" }, fmt(n.count(store.data))) : null));
  }
}

/* ---- inspector -------------------------------------------------------------
   One current concept, one place it is shown. A click anywhere — a figure on a
   drawing, a node on the graph, a row in a table — selects; nothing navigates
   on a single click, so a row no longer has two destinations depending on
   which pixel was hit. */
export function select(id, opts = {}) {
  const found = store.byId.get(id);
  if (!found) return;
  currentId = id;
  clear(inspectorActions);
  const where = { element: ["elements", "Elements"], relation: ["relations", "Relationships"], view: ["views", "Views"] }[found.kind];
  inspectorActions.append(
    where ? iconButton(where[0] === "views" ? "view" : where[0], `Find this in ${where[1]}`,
      () => { location.hash = href(found.kind, id); }, { variant: "quiet" }) : null,
  );
  renderConcept(inspectorBody, id);
  if (opts.focus !== false) inspectorBody.scrollTop = 0;
  document.dispatchEvent(new CustomEvent("amcli:select", { detail: { id } }));
}

export function clearSelection() {
  currentId = null;
  clear(inspectorActions);
  clear(inspectorBody).appendChild(h("div", { class: "empty" },
    h("p", { class: "empty-title" }, "Nothing selected"),
    h("p", { class: "empty-body" }, "Pick a row, a figure on a drawing or a box on the graph, and it will be described here.")));
  document.dispatchEvent(new CustomEvent("amcli:select", { detail: { id: null } }));
}

function applyInspector(narrow) {
  app.classList.toggle("inspector-narrow", narrow);
  prefs.set("amcli-inspector-narrow", narrow ? "1" : "0");
  clear(inspectorToggle).appendChild(icon("inspector"));
  inspectorToggle.title = narrow ? "Widen the details panel (⌘I)" : "Narrow the details panel (⌘I)";
  inspectorToggle.setAttribute("aria-label", inspectorToggle.title);
}
const inspectorToggle = document.getElementById("inspector-toggle");
inspectorToggle.addEventListener("click", () => applyInspector(!app.classList.contains("inspector-narrow")));
applyInspector(prefs.get("amcli-inspector-narrow") === "1");

export function selectedId() { return currentId; }

// Drag the seam. The width is remembered, because a reader who widened the
// panel to read documentation should not have to do it again on the next
// concept.
(function resizable() {
  const grip = document.getElementById("inspector-grip");
  const stored = parseInt(prefs.get("amcli-inspector-w", ""), 10);
  const clamp = (px) => {
    const min = num("--inspector-min"), max = num("--inspector-max");
    return Math.max(min, Math.min(max, px));
  };
  const num = (name) => parseInt(getComputedStyle(document.documentElement).getPropertyValue(name), 10) || 0;
  const setWidth = (px) => {
    app.style.setProperty("--inspector-w", `${clamp(px)}px`);
    prefs.set("amcli-inspector-w", String(clamp(px)));
  };
  if (stored) setWidth(stored);

  let from = null;
  grip.addEventListener("pointerdown", (e) => {
    from = { x: e.clientX, w: inspector.getBoundingClientRect().width };
    grip.setPointerCapture(e.pointerId);
    grip.classList.add("is-held");
    document.body.classList.add("is-resizing");
  });
  grip.addEventListener("pointermove", (e) => { if (from) setWidth(from.w + (from.x - e.clientX)); });
  const stop = () => { from = null; grip.classList.remove("is-held"); document.body.classList.remove("is-resizing"); };
  grip.addEventListener("pointerup", stop);
  grip.addEventListener("pointercancel", stop);
  grip.addEventListener("keydown", (e) => {
    const step = e.shiftKey ? num("--sp-12") : num("--sp-4");
    if (e.key === "ArrowLeft") { e.preventDefault(); setWidth(inspector.getBoundingClientRect().width + step); }
    if (e.key === "ArrowRight") { e.preventDefault(); setWidth(inspector.getBoundingClientRect().width - step); }
  });
})();

/* ---- status ----------------------------------------------------------------- */
function setStatus(kind, text, title) {
  statusEl.className = `status ${kind}`;
  statusEl.querySelector(".status-text").textContent = text;
  statusEl.title = title || "";
}

/* ---- keyboard ----------------------------------------------------------------
   One place, so a shortcut cannot mean two things on two pages. */
document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName || "");
  if (mod && e.key.toLowerCase() === "k") { e.preventDefault(); paletteIsOpen() ? closePalette() : openPalette(); return; }
  if (mod && e.key.toLowerCase() === "b") { e.preventDefault(); applyRail(!app.classList.contains("rail-collapsed")); return; }
  if (mod && e.key.toLowerCase() === "i") {
    e.preventDefault();
    applyInspector(!app.classList.contains("inspector-narrow"));
    return;
  }
  if (e.key === "Escape" && !paletteIsOpen() && currentId && !typing) { clearSelection(); return; }
  if (e.key === "/" && !typing && !paletteIsOpen()) {
    const box = main.querySelector(".field-input");
    if (box) { e.preventDefault(); box.focus(); box.select?.(); }
  }
});

/* ---- routing ------------------------------------------------------------------ */
function pageFor(route) {
  switch (route.page) {
    case "view": return viewPage;
    case "graph": return graph;
    case "stats": return stats;
    default: return collection;
  }
}

function render(route) {
  if (!store.data) return;
  unmount();
  clear(main);
  clear(railContextEl);
  const owner = OWNER[route.page] || route.page;
  nav.querySelectorAll("a").forEach((a) => a.classList.toggle("is-current", a.dataset.page === owner));

  // A concept's deep link is the collection it belongs to, with the concept
  // selected — not a second, wider copy of the inspector.
  const deep = (route.page === "element" || route.page === "relation") && route.id;
  const effective = deep ? { ...route, page: owner, id: null } : route;

  try {
    unmount = pageFor(effective).mount(main, effective) || (() => {});
  } catch (err) {
    console.error(err);
    main.appendChild(h("div", { class: "empty" },
      h("p", { class: "empty-title" }, "This page could not be drawn"),
      h("p", { class: "empty-body" }, err.message)));
    unmount = () => {};
  }
  if (deep) select(route.id);
}

function refreshShell() {
  const d = store.data;
  document.getElementById("model-name").textContent = d.model.name || "(unnamed model)";
  // The file's name, with the whole path on the hover: a path reversed to
  // put the ellipsis at the front read as `…chpad/demo/bank.archimate/`.
  const path = document.getElementById("model-path");
  path.textContent = d.model.path.split(/[\\/]/).pop();
  path.title = d.model.path;
  document.title = `${d.model.name || "amcli"} — amcli`;
  buildNav();
}

subscribe((event) => {
  if (event === "model") refreshShell();
  if (event === "changed") {
    setStatus("is-live is-changed", "updated", `Reloaded at ${new Date().toLocaleTimeString()}`);
    render(parse());
    if (currentId) select(currentId, { focus: false });
    setTimeout(() => setStatus("is-live", "live", `Watching ${store.data.model.path}`), 2500);
  } else if (event === "status") {
    if (store.error) setStatus("is-error", "file invalid", `The model file no longer parses; showing the last good version.\n${store.error}`);
    else setStatus("is-live", "live", `Watching ${store.data.model.path}`);
  } else if (event === "offline") {
    setStatus("is-error", "server gone", "amcli web is no longer answering — was it stopped?");
  }
});

document.querySelector(".rail-foot-row").append(h("span", { class: "spacer" }), themeBtn);

onRoute(render);

load()
  .then(() => {
    setStatus("is-live", "live", `Watching ${store.data.model.path}`);
    if (!location.hash) location.hash = href("views");
    clearSelection();
    render(parse());
    startPolling(2000);
  })
  .catch((e) => {
    setStatus("is-error", "failed", e.message);
    main.appendChild(h("div", { class: "empty" },
      h("p", { class: "empty-title" }, "Could not load the model"),
      h("p", { class: "empty-body" }, e.message)));
  });
