// Views: the folder tree on the left, a table of what is in the chosen folder
// on the right; a click opens the drawing full width — the very SVG the server
// renders from the file, so what is on screen is what Archi would draw, and a
// click on a figure opens the concept it stands for.
//
// The tree, not a row of chips, because a real model files its views several
// folders deep: eighty-six views under twenty-five folders came out as
// twenty-five chips carrying their whole path, which wrapped over four lines
// and still could not say that A1 sits inside A. A tree says it in its shape,
// scrolls on its own, and lets a whole branch be picked at once.

import { h, clear, fmt } from "../dom.js";
import { store, view, folder, folderPath } from "../store.js";
import { href, replaceParams } from "../router.js";
import { attachPanZoom } from "../panzoom.js";
import { select } from "../app.js";

// Which branches are open, kept for the session so leaving a view and coming
// back does not collapse the tree the reader just arranged.
const collapsed = new Set();
let treeInitialised = false;

// How the table was last left — folder, filter, sort. Opening a drawing and
// coming back should land on the rows the reader opened it from, not at the
// top of all eighty-six of them.
let lastList = {};

export function mount(main, route) {
  if (route.page === "view" && route.id) {
    const found = store.byId.get(route.id);
    if (found && found.kind === "view") return renderView(main, found.i, route.params.get("focus"));
    const page = h("div", { class: "page" }, h("div", { class: "page-head" }, h("a", { class: "btn sm", href: href("views", null, lastList) }, "‹ Views")), h("div", { class: "empty" }, `No view has id ${route.id}`));
    main.appendChild(page);
    return () => {};
  }
  return renderTable(main, route);
}

// ---- the folder tree -------------------------------------------------------

// Every view in a folder and everything under it, folder by folder. A folder
// with nothing below it is left out: the views page is for finding a drawing,
// and a branch that holds none is only in the way.
function subtrees() {
  const kids = store.folderKids;
  const all = new Map();
  const walk = (i) => {
    const list = store.folderViews[i].slice();
    for (const k of kids[i]) list.push(...walk(k));
    all.set(i, list);
    return list;
  };
  for (const r of store.roots) walk(r);
  return all;
}

// ---- the table -------------------------------------------------------------

function renderTable(main, route) {
  const page = h("div", { class: "page" });
  const head = h("div", { class: "page-head" });
  const split = h("div", { class: "split" });
  const treePane = h("div", { class: "pane tree-pane" });
  const tablePane = h("div", { class: "pane" });
  split.append(treePane, tablePane);
  page.append(head, split);
  main.appendChild(page);

  const p = route.params;
  const state = { folder: p.get("folder") || "", q: p.get("q") || "", sort: p.get("sort") || "name", dir: p.get("dir") || "asc" };
  const push = () => replaceParams(remember());
  const remember = () => (lastList = { folder: state.folder, q: state.q, sort: state.sort === "name" ? "" : state.sort, dir: state.dir === "asc" ? "" : state.dir });
  remember();

  const all = store.data.views;
  const under = subtrees();
  const roots = store.roots.filter((i) => under.get(i).length);
  // Deep trees start folded past the top level; a small one is opened whole,
  // because scrolling a dozen rows beats clicking a dozen twisties.
  if (!treeInitialised) {
    treeInitialised = true;
    if (under.size > 40) for (const [i, list] of under) if (list.length && folder(i).parent !== null) collapsed.add(folderPath(i));
  }

  const q = h("input", { class: "input", type: "search", placeholder: "Filter by name…", value: state.q, style: { width: "220px" },
    oninput: (e) => { state.q = e.target.value; push(); renderTree(); renderRows(); } });
  const crumb = h("span", { class: "muted small ellipsis", style: { maxWidth: "40%" } });
  const summary = h("span", { class: "muted small nowrap" });
  head.append(q, crumb, h("span", { class: "spacer" }), summary);
  const table = h("table", { class: "grid" });
  tablePane.appendChild(table);

  // A view is in the chosen folder when it is in that folder or under it, so
  // picking "A. Vision and Compliance" gets everything filed below it too.
  const inFolder = (v) => {
    if (!state.folder) return true;
    const f = folderPath(v.folder);
    return f === state.folder || f.startsWith(state.folder + "/");
  };
  const matches = (v) => {
    const needle = state.q.trim().toLowerCase();
    return !needle || (v.name || "").toLowerCase().includes(needle);
  };

  function pick(path) {
    state.folder = state.folder === path ? "" : path;
    push();
    renderTree();
    renderRows();
  }

  function renderTree() {
    clear(treePane);
    const hits = new Set();
    all.forEach((v, i) => { if (matches(v)) hits.add(i); });
    const count = (i) => under.get(i).reduce((n, vi) => n + (hits.has(vi) ? 1 : 0), 0);

    treePane.appendChild(row({ label: "All views", n: hits.size, path: "", depth: 0, kids: [] }));
    const add = (i, depth) => {
      const path = folderPath(i);
      const branches = store.folderKids[i].filter((k) => under.get(k).length);
      treePane.appendChild(row({ label: folder(i).name, n: count(i), path, depth, kids: branches }));
      if (!collapsed.has(path)) for (const k of branches) add(k, depth + 1);
    };
    for (const r of roots) add(r, 1);
  }

  function row({ label, n, path, depth, kids }) {
    // A folder name is long enough to be cut off in a pane this narrow, so
    // the whole of it — and where it sits — is on the hover.
    const el = h("div", {
      class: "tree-row" + (state.folder === path ? " active" : "") + (n ? "" : " empty-branch"),
      style: { paddingLeft: `${depth * 13}px` },
      title: path ? `${label}\n${path}` : "Every view in the model",
      onclick: () => pick(path),
    });
    el.append(
      kids.length
        ? h("button", { class: "twisty", title: collapsed.has(path) ? "Expand" : "Collapse",
            onclick: (e) => { e.stopPropagation(); if (collapsed.has(path)) collapsed.delete(path); else collapsed.add(path); renderTree(); } },
            collapsed.has(path) ? "▶" : "▼")
        : h("span", { class: "twisty" }),
      h("span", { class: "label" }, label),
      h("span", { class: "n" }, fmt(n)),
    );
    return el;
  }

  const key = {
    name: (i) => view(i).name.toLowerCase(),
    folder: (i) => folderPath(view(i).folder),
    viewpoint: (i) => view(i).viewpoint,
    elements: (i) => view(i).elements.length,
    relations: (i) => view(i).relations.length,
  };
  function th(label, k, cls) {
    return h("th", {
      class: (cls || "") + (state.sort === k ? " sorted" + (state.dir === "asc" ? " asc" : "") : ""),
      onclick: () => {
        if (state.sort === k) state.dir = state.dir === "asc" ? "desc" : "asc";
        else { state.sort = k; state.dir = ["elements", "relations"].includes(k) ? "desc" : "asc"; }
        push(); renderRows();
      },
    }, label);
  }

  function renderRows() {
    const list = [];
    for (let i = 0; i < all.length; i++) if (inFolder(all[i]) && matches(all[i])) list.push(i);
    const numeric = ["elements", "relations"].includes(state.sort);
    const k = key[state.sort] || key.name;
    list.sort((a, b) => {
      const c = numeric ? k(a) - k(b) : String(k(a)).localeCompare(String(k(b)), undefined, { numeric: true });
      return state.dir === "asc" ? c : -c;
    });
    summary.textContent = `${fmt(list.length)} of ${fmt(all.length)}`;
    crumb.textContent = state.folder || "";
    crumb.title = state.folder || "";
    clear(table);
    if (!list.length) {
      table.appendChild(h("tbody", null, h("tr", null, h("td", { colspan: 5 }, h("div", { class: "empty" },
        all.length ? "No views match." : "This model has no views yet — amcli view auto \"Name\" --from <element> draws one.")))));
      return;
    }
    table.appendChild(h("thead", null, h("tr", null, th("Name", "name"), th("Folder", "folder"), th("Viewpoint", "viewpoint"), th("Elements", "elements", "num"), th("Relationships", "relations", "num"))));
    const tbody = h("tbody");
    for (const i of list) {
      const v = view(i);
      const tr = h("tr", null,
        h("td", null, h("a", { href: href("view", v.id) }, "▣ ", v.name)),
        h("td", { class: "muted" }, relativePath(folderPath(v.folder), state.folder)),
        h("td", { class: "muted" }, v.viewpoint || ""),
        h("td", { class: "num" }, v.elements.length),
        h("td", { class: "num" }, v.relations.length),
      );
      tr.addEventListener("click", () => { location.hash = href("view", v.id); });
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
  }

  renderTree();
  renderRows();
  return () => {};
}

// The part of a folder's path the chosen folder does not already say. The
// column is there to place a view within the current branch, and repeating
// "/Views/A. Vision and Compliance/" on every row places nothing.
function relativePath(path, base) {
  const rest = base && path.startsWith(base) ? path.slice(base.length) : path.replace(/^\/Views/, "");
  return rest.replace(/^\//, "") || (base ? "" : "/");
}

// ---- one view --------------------------------------------------------------

function renderView(main, vi, focus) {
  const v = view(vi);
  const page = h("div", { class: "page" });
  main.appendChild(page);

  // Every view, grouped by folder, so switching drawings is one control away.
  const picker = h("select", { class: "input", style: { maxWidth: "320px" }, onchange: (e) => { location.hash = href("view", e.target.value); } });
  const byFolder = new Map();
  store.data.views.forEach((w, i) => { const f = folderPath(w.folder); if (!byFolder.has(f)) byFolder.set(f, []); byFolder.get(f).push(i); });
  for (const f of [...byFolder.keys()].sort()) {
    const grp = h("optgroup", { label: f.replace(/^\/Views\/?/, "") || "/Views" });
    for (const i of byFolder.get(f).sort((a, b) => view(a).name.localeCompare(view(b).name, undefined, { numeric: true }))) {
      grp.appendChild(h("option", { value: view(i).id, selected: i === vi }, view(i).name));
    }
    picker.appendChild(grp);
  }

  const head = h("div", { class: "page-head" },
    h("a", { class: "btn sm", href: href("views", null, lastList), title: "Back to the list" }, "‹ Views"),
    picker,
    v.viewpoint ? h("span", { class: "badge" }, v.viewpoint) : null,
    h("span", { class: "muted small" }, `${fmt(v.elements.length)} elements · ${fmt(v.relations.length)} relationships · ${folderPath(v.folder)}`),
    h("span", { class: "spacer" }),
    h("a", { class: "btn sm", href: `/api/view/${encodeURIComponent(v.id)}.svg`, target: "_blank", rel: "noopener", title: "Open the SVG" }, "SVG ↗"),
    h("a", { class: "btn sm", href: `/api/view/${encodeURIComponent(v.id)}.png`, download: `${safeName(v.name)}.png`, title: "Download as PNG (2× resolution)" }, "PNG ↓"),
  );
  const canvas = h("div", { class: "canvas" });
  const hud = h("div", { class: "canvas-hud" });
  page.append(head, canvas);
  canvas.appendChild(hud);

  let pz = null;
  let alive = true;
  const msg = h("div", { class: "canvas-msg" }, "Rendering…");
  canvas.appendChild(msg);

  const url = `/api/view/${encodeURIComponent(v.id)}.svg?c=${encodeURIComponent(store.checksum)}`;
  fetch(url, { cache: "no-store" })
    .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`HTTP ${r.status}`))))
    .then((text) => {
      if (!alive) return;
      const doc = new DOMParser().parseFromString(text, "image/svg+xml");
      const svg = document.adoptNode(doc.documentElement);
      if (svg.nodeName.toLowerCase() !== "svg") throw new Error("not an SVG");
      msg.remove();
      const vb = (svg.getAttribute("viewBox") || "0 0 100 100").split(/\s+/).map(Number);
      const box = { x: vb[0], y: vb[1], w: vb[2], h: vb[3] };
      svg.removeAttribute("width");
      svg.removeAttribute("height");
      svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
      canvas.insertBefore(svg, hud);
      pz = attachPanZoom(svg, canvas, { maxFitScale: 1.25 });
      pz.fit(box);
      hud.append(
        h("button", { class: "btn sm", onclick: () => pz.fit(box), title: "Fit to window" }, "Fit"),
        h("button", { class: "btn sm", onclick: () => pz.actual(), title: "Actual size" }, "1:1"),
        h("button", { class: "btn sm", onclick: () => pz.zoomIn() }, "+"),
        h("button", { class: "btn sm", onclick: () => pz.zoomOut() }, "−"),
      );
      wire(svg, canvas, focus);
      if (svg.querySelectorAll("[data-concept]").length === 0 && v.elements.length === 0) {
        canvas.appendChild(h("div", { class: "canvas-msg" }, "This view is empty."));
      }
    })
    .catch((e) => {
      if (!alive) return;
      msg.textContent = `Could not render this view: ${e.message}`;
    });

  return () => { alive = false; pz?.destroy(); };
}

// Clicks on figures and connections open the concept; the current one is
// outlined; a double-click goes on to the graph centred there, the same jump
// the details panel's "Open in graph" makes. Groups nested inside groups: the
// innermost wins.
function wire(svg, canvas, focus) {
  let selected = null;
  const mark = (g) => {
    selected?.classList.remove("selected");
    selected = g;
    g?.classList.add("selected");
  };
  svg.addEventListener("click", (e) => {
    if (canvas.dataset.justDragged) return;
    const g = e.target.closest("[data-concept], [data-relationship]");
    if (!g) { mark(null); return; }
    const id = g.dataset.concept || g.dataset.relationship;
    mark(g);
    select(id);
  });
  svg.addEventListener("dblclick", (e) => {
    const g = e.target.closest("[data-concept]");
    if (!g) return;
    e.preventDefault();
    location.hash = href("graph", null, { focus: g.dataset.concept, depth: 1 });
  });
  if (focus) {
    const g = svg.querySelector(`[data-concept="${cssEscape(focus)}"], [data-relationship="${cssEscape(focus)}"]`);
    if (g) { mark(g); select(focus); }
  }
}

function safeName(name) {
  return (name || "view").replace(/[\\/:*?"<>|]+/g, "_").trim() || "view";
}

function cssEscape(v) {
  return (window.CSS && CSS.escape) ? CSS.escape(v) : String(v).replace(/["\\]/g, "\\$&");
}
