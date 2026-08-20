// Views, Elements and Relationships — one page, three configurations.
//
// They used to be three modules with three sortable-table headers (three
// different rules about which columns sort descending first), three
// filter→sort→cap→render pipelines and three ways of filtering: a folder tree
// here, a flat forty-one-entry `<select>` there, a row of chips on the third.
// Nothing about a collection of concepts differs between them except which
// columns to show and which dimensions to offer, so that — and only that — is
// what a spec says below.

import { h, clear, fmt, relLabel, esc } from "../dom.js";
import { store, elem, rel, view, folder, folderPath } from "../store.js";
import { matches as looksLike, mark } from "../fuzzy.js";
import { typeIcon, typeOf, accessLabel } from "../notation.js";
import { icon } from "../icons.js";
import { toolbar, filterBar, dataTable, tree, searchField, countLabel } from "../ui.js";
import { href, replaceParams } from "../router.js";
import { select, selectedId, railContext } from "../app.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];

// Which branches are folded, and how each collection was last left. Kept for
// the session so that opening a drawing and coming back lands on the rows it
// was opened from rather than at the top of all eighty-six.
const folded = new Map();   // kind → Set of folder paths
const lastLeft = new Map(); // kind → params

const cell = (...kids) => h("span", { class: "cell" }, kids);

// What the search box held when the table was last painted. A column's render
// takes a row, not a query — and a fuzzy match that does not say which letters
// it matched is a table full of rows the reader cannot account for, so the one
// piece of state the cells need is kept here rather than threaded through
// every render signature.
let typed = "";
const hit = (text) => mark(text, typed);

/* ---- the three configurations ---------------------------------------------- */

function specFor(kind) {
  if (kind === "views") return {
    kind, title: "Views", iconName: "view", noun: "views",
    all: () => store.data.views.map((_, i) => i),
    at: view,
    id: (i) => view(i).id,
    name: (i) => view(i).name || "",
    folderIndex: (i) => view(i).folder,
    inFolders: store.folderViews,
    open: (i) => (location.hash = href("view", view(i).id)),
    columns: [
      { key: "name", label: "Name", width: "30%", sortable: true, sort: (i) => name(view(i)), render: (i) => cell(icon("view"), h("span", { class: "ellipsis" }, hit(view(i).name || "(unnamed)"))) },
      { key: "folder", label: "Folder", width: "22%", sortable: true, cls: "path", sort: (i) => folderPath(view(i).folder), render: (i) => titled(trimPath(folderPath(view(i).folder), "/Views"), folderPath(view(i).folder)) },
      { key: "viewpoint", label: "Viewpoint", width: "15%", sortable: true, sort: (i) => view(i).viewpoint || "", render: (i) => view(i).viewpoint || h("span", { class: "subtle" }, "—") },
      { key: "elements", label: "Elements", width: "14%", sortable: true, numeric: true, align: "right", sort: (i) => view(i).elements.length, render: (i) => fmt(view(i).elements.length) },
      { key: "relations", label: "Relationships", width: "19%", sortable: true, numeric: true, align: "right", sort: (i) => view(i).relations.length, render: (i) => fmt(view(i).relations.length) },
    ],
    dimensions: [{
      key: "viewpoint", label: "Viewpoints", noun: "viewpoints",
      valuesOf: () => tally(store.data.views.map((v) => v.viewpoint || "(none)")),
      of: (i) => view(i).viewpoint || "(none)",
    }],
    matches: (i, q) => looksLike(q, view(i).name || ""),
    // The drawings themselves hang in the tree their folders are in. A folder
    // list beside a list of what is in the folders is the same list twice, and
    // a model's views are where a reader looks for a drawing by name — it is
    // the shape Archi files them in.
    leaves: { icon: () => icon("view"), of: (id) => (store.byId.get(id)?.kind === "view" ? store.byId.get(id).i : null) },
    emptyTitle: "No view matches",
    emptyBody: "Clear the filter, or draw one with `amcli view auto \"Name\" --from <element>`.",
  };

  if (kind === "relations") return {
    kind, title: "Relationships", iconName: "relations", noun: "relationships",
    all: () => store.data.relations.map((_, i) => i),
    at: rel,
    id: (i) => rel(i).id,
    name: (i) => rel(i).name || "",
    folderIndex: (i) => rel(i).folder,
    inFolders: store.folderRels,
    open: (i) => {
      const r = rel(i);
      if (r.src >= 0) location.hash = href("graph", null, { focus: elem(r.src).id, depth: 1 });
    },
    columns: [
      { key: "type", label: "Kind", width: "15%", sortable: true, sort: (i) => rel(i).type, render: (i) => cell(icon("relations"), h("span", { class: "ellipsis" }, relLabel(rel(i).type))) },
      { key: "source", label: "Source", width: "21%", sortable: true, sort: (i) => endName(rel(i).src).toLowerCase(), render: (i) => endCell(rel(i).src, rel(i).srcId) },
      { key: "arrow", label: "", width: "3%", cls: "cell-arrow", render: () => icon("arrow-right", { class: "rel-arrow" }) },
      { key: "target", label: "Target", width: "21%", sortable: true, sort: (i) => endName(rel(i).tgt).toLowerCase(), render: (i) => endCell(rel(i).tgt, rel(i).tgtId) },
      { key: "name", label: "Label", width: "14%", sortable: true, sort: (i) => (rel(i).name || "").toLowerCase(), render: (i) => (rel(i).name ? hit(rel(i).name) : detailOf(rel(i)) || h("span", { class: "subtle" }, "—")) },
      { key: "folder", label: "Folder", width: "16%", sortable: true, cls: "path", sort: (i) => folderPath(rel(i).folder), render: (i) => titled(trimPath(folderPath(rel(i).folder), ""), folderPath(rel(i).folder)) },
      { key: "views", label: "Views", width: "10%", sortable: true, numeric: true, align: "right", sort: (i) => store.viewsOfRel[i].length, render: (i) => fmt(store.viewsOfRel[i].length) },
    ],
    // Keyed "kind", not "type": the graph reserves `no_type` for element types
    // and writes `no_kind` for these, and one param name cannot mean two
    // things. The sortable column keyed "type" above is a separate namespace.
    dimensions: [{
      key: "kind", label: "Kinds", noun: "kinds",
      valuesOf: () => tally(store.data.relations.map((r) => r.type), relLabel),
      of: (i) => rel(i).type,
    }],
    defaultSort: "source",
    matches: (i, q) => {
      const r = rel(i);
      return looksLike(q, r.name || "") || looksLike(q, endName(r.src)) || looksLike(q, endName(r.tgt));
    },
    emptyTitle: "No relationship matches",
    emptyBody: "Clear the filter or show more kinds.",
  };

  return {
    kind: "elements", title: "Elements", iconName: "elements", noun: "elements",
    all: () => store.data.elements.map((_, i) => i),
    at: elem,
    id: (i) => elem(i).id,
    name: (i) => elem(i).name || "",
    folderIndex: (i) => elem(i).folder,
    inFolders: store.folderElems,
    open: (i) => (location.hash = href("graph", null, { focus: elem(i).id, depth: 1 })),
    columns: [
      { key: "name", label: "Name", width: "29%", sortable: true, sort: (i) => name(elem(i)), render: (i) => cell(typeIcon(elem(i).type), h("span", { class: "ellipsis" }, hit(elem(i).name || "(unnamed)"))) },
      { key: "type", label: "Type", width: "23%", sortable: true, sort: (i) => elem(i).type, render: (i) => cell(h("span", { class: "swatch", title: `${elem(i).layer} layer`, style: { background: typeOf(elem(i).type).fill } }), elem(i).type) },
      { key: "folder", label: "Folder", width: "21%", sortable: true, cls: "path", sort: (i) => folderPath(elem(i).folder), render: (i) => titled(trimPath(folderPath(elem(i).folder), ""), folderPath(elem(i).folder)) },
      { key: "in", label: "In", width: "8%", sortable: true, numeric: true, align: "right", sort: (i) => store.inc[i].length, render: (i) => fmt(store.inc[i].length) },
      { key: "out", label: "Out", width: "8%", sortable: true, numeric: true, align: "right", sort: (i) => store.out[i].length, render: (i) => fmt(store.out[i].length) },
      { key: "views", label: "Views", width: "11%", sortable: true, numeric: true, align: "right", sort: (i) => store.viewsOfElem[i].length, render: (i) => fmt(store.viewsOfElem[i].length) },
    ],
    dimensions: [
      {
        key: "layer", label: "Layers", noun: "layers",
        valuesOf: () => tally(store.data.elements.map((e) => e.layer), null, LAYERS, (l) => layerFill(l)),
        of: (i) => elem(i).layer,
      },
      {
        key: "type", label: "Types", noun: "types",
        valuesOf: (hidden) => tally(
          store.data.elements.filter((e) => !hidden.layer?.has(e.layer)).map((e) => e.type),
          null, null, (t) => typeOf(t).fill),
        of: (i) => elem(i).type,
      },
    ],
    matches: (i, q) => looksLike(q, elem(i).name || ""),
    emptyTitle: "No element matches",
    emptyBody: "Clear the filter, or show more layers and types.",
  };
}

const name = (c) => (c.name || "").toLowerCase();
const endName = (i) => (i >= 0 ? elem(i).name || "" : "");
const endCell = (i, raw) => i >= 0
  ? cell(typeIcon(elem(i).type), h("span", { class: "ellipsis" }, hit(elem(i).name || "(unnamed)")))
  : h("span", { class: "subtle mono" }, raw || "—");
const detailOf = (r) =>
  r.type === "Access" && r.access !== null ? accessLabel(r.access)
    : r.type === "Association" && r.directed ? "directed" : "";

function layerFill(layer) {
  for (const t of Object.values(store.data.types)) if (t.layer === layer) return t.fill;
  return null;
}

// value → {value,label,count,swatch}, in a stable order.
function tally(values, label, order, fill) {
  const n = new Map();
  for (const v of values) n.set(v, (n.get(v) || 0) + 1);
  const keys = order ? order.filter((k) => n.has(k)) : [...n.keys()].sort();
  return keys.map((k) => ({ value: k, label: label ? label(k) : k, count: n.get(k), swatch: fill ? fill(k) : null }));
}

// A cell that is going to be cut has the whole of it on the hover.
function titled(text, full) {
  return h("span", { class: "ellipsis", title: full }, text);
}

function trimPath(path, base) {
  const rest = base && path.startsWith(base) ? path.slice(base.length) : path;
  return rest.replace(/^\//, "") || "/";
}

// Share a dropped column's width out again, so the shares still add up to the
// whole table. It goes to the columns holding text, in proportion to what they
// already have: a count column is as wide as it will ever need to be, and the
// name is the one that was being cut.
function widen(columns, by) {
  const extra = parseFloat(by);
  const text = columns.filter((c) => !c.numeric);
  const room = text.reduce((n, c) => n + parseFloat(c.width), 0);
  if (!extra || !room) return columns;
  const grow = 1 + extra / room;
  return columns.map((c) => (c.numeric ? c : { ...c, width: `${(parseFloat(c.width) * grow).toFixed(1)}%` }));
}

/* ---- the page --------------------------------------------------------------- */

export function mount(main, route) {
  const spec = specFor(route.page === "views" || route.page === "view" ? "views"
    : route.page === "relations" || route.page === "relation" ? "relations" : "elements");
  const p = route.params;

  if (!folded.has(spec.kind)) folded.set(spec.kind, new Set());
  const collapsed = folded.get(spec.kind);

  const hidden = {};
  for (const d of spec.dimensions) hidden[d.key] = new Set((p.get(`no_${d.key}`) || "").split(",").filter(Boolean));

  const state = {
    folder: p.get("folder") || "",
    q: p.get("q") || "",
    sort: p.get("sort") || spec.defaultSort || spec.columns[0].key,
    dir: p.get("dir") || "asc",
  };

  const push = () => {
    const params = {
      folder: state.folder, q: state.q,
      sort: state.sort === (spec.defaultSort || spec.columns[0].key) ? "" : state.sort,
      dir: state.dir === "asc" ? "" : state.dir,
    };
    for (const d of spec.dimensions) params[`no_${d.key}`] = [...hidden[d.key]].join(",");
    lastLeft.set(spec.kind, params);
    replaceParams(params);
  };
  push();

  // ---- structure
  const page = h("div", { class: "page" });
  // How many rows are showing goes in the toolbar's meta slot, beside the
  // title it qualifies — not at the far right, where it reads as a control.
  const count = h("span", { class: "num" });
  const field = searchField({
    value: state.q, placeholder: `Filter ${spec.noun}…`, width: "var(--ctl-w)",
    oninput: (v) => { state.q = v; push(); redraw(); },
  });
  const bar = toolbar({
    title: spec.title, titleIcon: spec.iconName, meta: count,
    controls: [field],
  });

  const dims = spec.dimensions.map((d) => ({
    key: d.key, label: d.label, noun: d.noun, hidden: hidden[d.key],
    values: () => d.valuesOf(hidden),
    onChange: () => { push(); filters.redraw(); redraw(); },
  }));
  const filters = filterBar(dims);

  const tablePane = h("div", { class: "page-body" });
  page.append(bar, tablePane);
  main.appendChild(page);

  // The tree and the filters go in the rail, not in a band across the page:
  // the space under the menu was doing nothing, and narrowing the view is one
  // job that belongs in one place.
  const rail = railContext();
  const treePane = h("div", { class: "rail-scope" });
  rail.append(
    spec.dimensions.length ? h("div", { class: "rail-group" }, h("h2", { class: "caps rail-group-title" }, "Filter"), filters) : null,
    h("div", { class: "rail-group rail-group-grow" }, h("h2", { class: "caps rail-group-title" }, "Folders"), treePane));

  // ---- the folder tree
  // Every hierarchical dimension is a tree, and always this one — the folder
  // of an element was a flat forty-one-entry select on one page and a tree on
  // another, for the same data.
  const under = subtreeCounts(spec.inFolders);
  const roots = store.roots.filter((i) => under.get(i).length);
  const hasHierarchy = roots.length > 0 && (roots.length > 1 || store.folderKids[roots[0]].some((k) => under.get(k).length));
  if (!hasHierarchy) treePane.closest(".rail-group")?.remove();

  const soleRoot = soleRootOf(roots, under, spec.all().length);
  // A link written while that row still existed can still ask for it.
  if (soleRoot !== null && state.folder === folderPath(soleRoot)) { state.folder = ""; push(); }

  // How many rows the tree would open with, leaves included: a folder list is
  // worth reading whole, eighty-six drawings under it are not.
  if (hasHierarchy && !collapsed.size && under.size + (spec.leaves ? spec.all().length : 0) > 24) {
    for (const [i, list] of under) if (list.length && folder(i).parent !== null) collapsed.add(folderPath(i));
  }

  const treeNodes = (passing) => folderNodes({ spec, under, roots, soleRoot, collapsed, hits: new Set(passing) });

  // A leaf's key is its index behind a `#`, which no folder path can be.
  const atKey = (k) => (k.startsWith("#") ? +k.slice(1) : null);
  const keyOf = (id) => {
    const found = spec.leaves && id ? store.byId.get(id) : null;
    return found && spec.leaves.of(id) !== null ? `#${found.i}` : null;
  };

  function drawTree(passing) {
    // Expanding a folder or picking one rebuilds the whole tree, and the row
    // that was rebuilt is the row the keyboard was standing on. Without this
    // focus lands back on <body> and the tree has to be tabbed into again from
    // the top of the rail for every single folder. The scroll goes the same
    // way and for the same reason: a tree that holds every drawing is a tree
    // you are somewhere in the middle of.
    const held = treePane.contains(document.activeElement)
      ? document.activeElement.closest(".tree-row")?.dataset.key
      : null;
    const at = treePane.scrollTop;
    clear(treePane);
    if (!hasHierarchy) return;
    treePane.appendChild(tree({
      nodes: treeNodes(passing),
      active: state.folder,
      selected: keyOf(selectedId()),
      label: "Folders",
      isOpen: (k) => !collapsed.has(k),
      onToggle: (k) => { collapsed.has(k) ? collapsed.delete(k) : collapsed.add(k); drawTree(passing); },
      onOpen: (k) => { const i = atKey(k); if (i !== null) spec.open(i); },
      onPick: (k) => {
        // A leaf is a concept, and a concept is selected — the same single
        // click a row, a figure and a graph node get. Nothing navigates on it.
        const i = atKey(k);
        if (i !== null) { select(spec.id(i)); return; }
        if (state.folder === k && k) {
          state.folder = "";           // clicking the chosen folder again clears it
        } else {
          state.folder = k;
          collapsed.delete(k);         // and opening one shows what is inside it
        }
        push();
        redraw();
      },
    }));
    treePane.scrollTop = at;
    if (held != null) {
      const row = treePane.querySelector(`.tree-row[data-key="${esc(held)}"]`);
      if (row) { treePane.querySelectorAll(".tree-row").forEach((r) => r.setAttribute("tabindex", "-1")); row.setAttribute("tabindex", "0"); row.focus(); }
    }
  }

  // ---- the table
  // With no folder structure, `trimPath` has nothing left to return but "/",
  // so the Folder column is one repeated character on every row. The rail
  // drops its tree for the same reason and the column goes with it, giving
  // its width back to the columns holding names. The full path is still on
  // the name's hover and in the inspector.
  const columns = hasHierarchy ? spec.columns
    : widen(spec.columns.filter((c) => c.key !== "folder"), spec.columns.find((c) => c.key === "folder")?.width);
  // A link kept from a foldered model can ask for a sort by a column this one
  // does not have.
  if (!columns.some((c) => c.key === state.sort)) { state.sort = spec.defaultSort || columns[0].key; push(); }

  const table = dataTable({
    columns: columns.map((c) => ({ ...c, render: (row) => c.render(row.i) })),
    rows: [],
    id: (row) => spec.id(row.i),
    sort: { key: state.sort, dir: state.dir },
    onSort: (s) => { state.sort = s.key; state.dir = s.dir; push(); redraw(); },
    onSelect: (row) => select(spec.id(row.i)),
    onOpen: (row) => spec.open(row.i),
    emptyTitle: spec.emptyTitle,
    emptyBody: spec.emptyBody,
  });
  tablePane.appendChild(table.el);

  // ---- filtering
  const inFolder = (i) => {
    if (!state.folder) return true;
    const f = folderPath(spec.folderIndex(i));
    return f === state.folder || f.startsWith(state.folder + "/");
  };
  const passesDims = (i) => spec.dimensions.every((d) => !hidden[d.key].has(d.of(i)));

  // What the tree counts is everything the *other* filters let through — a
  // folder's number should say what is in it under the current filter, not
  // what would be in it if you also picked that folder.
  function passingForTree() {
    const q = state.q.trim();
    return spec.all().filter((i) => passesDims(i) && (!q || spec.matches(i, q)));
  }

  // Both `table.sort` and `table.rows` repaint the whole table, so setting the
  // two back to back rebuilt every cell twice for a change that only ever
  // touches one of them. The order only changes when a header is clicked; the
  // rows change on every letter typed into the filter, and at the thousand-row
  // cap that is the difference between keeping up with a typist and not.
  let painted = { key: state.sort, dir: state.dir };

  function redraw() {
    const q = state.q.trim();
    typed = q;
    const all = spec.all();
    const rows = all.filter((i) => passesDims(i) && inFolder(i) && (!q || spec.matches(i, q)));
    const col = columns.find((c) => c.key === state.sort) || columns[0];
    const key = col.sort || ((i) => spec.name(i).toLowerCase());
    rows.sort((a, b) => {
      const ka = key(a), kb = key(b);
      const c = typeof ka === "number" ? ka - kb : String(ka).localeCompare(String(kb), undefined, { numeric: true });
      return state.dir === "asc" ? c : -c;
    });
    if (painted.key !== state.sort || painted.dir !== state.dir) {
      painted = { key: state.sort, dir: state.dir };
      table.sort = painted;
    }
    table.rows = rows.map((i) => ({ i }));
    if (selectedId()) table.setSelected(selectedId(), { reveal: false });
    count.textContent = countLabel(rows.length, all.length);
    drawTree(passingForTree());
  }

  const onSelect = (e) => {
    table.setSelected(e.detail.id);
    // The mark moves; the tree does not get rebuilt for it. Rebuilding would
    // throw away where the reader is scrolled to for a class name.
    const key = keyOf(e.detail.id);
    for (const row of treePane.querySelectorAll(".tree-row.is-selected")) {
      row.classList.remove("is-selected");
      row.setAttribute("aria-selected", String(row.classList.contains("is-active")));
    }
    if (!key) return;
    const row = treePane.querySelector(`.tree-row[data-key="${esc(key)}"]`);
    if (row) { row.classList.add("is-selected"); row.setAttribute("aria-selected", "true"); row.scrollIntoView({ block: "nearest" }); }
  };
  document.addEventListener("amcli:select", onSelect);

  redraw();
  return () => {
    bar.destroy();
    document.removeEventListener("amcli:select", onSelect);
  };
}

/* ---- the tree both rails draw --------------------------------------------
   One shape, one place. The Views list narrows a table with this tree and a
   drawing navigates with it; when they were two pieces of code they became two
   shapes — a tree of folders here, and beside it a flat list of the drawings
   in the chosen one, which is the same list twice. Where a spec declares
   `leaves`, what is filed in a folder hangs off it. */
function folderNodes({ spec, under, roots, soleRoot, collapsed, hits }) {
  const count = (i) => under.get(i).reduce((n, x) => n + (hits.has(x) ? 1 : 0), 0);
  const nodes = [{ key: "", label: `All ${spec.noun}`, count: hits.size, depth: 0, hasKids: false, title: `Every one of the ${spec.noun} in the model` }];
  // What is filed directly in this folder and survives the filters, by name.
  const own = (i) => (spec.leaves ? spec.inFolders[i].filter((x) => hits.has(x)) : [])
    .sort((a, b) => spec.name(a).localeCompare(spec.name(b), undefined, { numeric: true }));
  const leaf = (x, depth) => nodes.push({
    key: `#${x}`, label: spec.name(x) || "(unnamed)", depth,
    hasKids: false, icon: spec.leaves.icon(x), title: spec.name(x),
  });
  const walk = (i, depth) => {
    const path = folderPath(i);
    const kids = store.folderKids[i].filter((k) => under.get(k).length);
    const mine = own(i);
    nodes.push({ key: path, label: folder(i).name, count: count(i), depth, hasKids: kids.length > 0 || mine.length > 0, title: `${folder(i).name}\n${path}` });
    if (collapsed.has(path)) return;
    for (const k of kids) walk(k, depth + 1);
    for (const x of mine) leaf(x, depth + 1);
  };
  const top = soleRoot === null ? roots : store.folderKids[soleRoot].filter((k) => under.get(k).length);
  for (const r of top) walk(r, 1);
  // The merged root's own items have nowhere else to go: they belong to the
  // "All …" row that stands in for it.
  if (soleRoot !== null) for (const x of own(soleRoot)) leaf(x, 1);
  return nodes;
}

// A model files every view under one top-level folder called "Views", and every
// relationship under one called "Relations". A root that holds the whole
// collection says exactly what the "All …" row above it says, so it is not
// drawn twice; its folders hang off that row instead.
const soleRootOf = (roots, under, total) =>
  (roots.length === 1 && under.get(roots[0]).length === total ? roots[0] : null);

// Folder index → every item in it and everything under it.
function subtreeCounts(byFolder) {
  const all = new Map();
  const walk = (i) => {
    const list = byFolder[i].slice();
    for (const k of store.folderKids[i]) list.push(...walk(k));
    all.set(i, list);
    return list;
  };
  for (const r of store.roots) walk(r);
  return all;
}


/* ---- the rail on a single drawing -------------------------------------------
   Opening a view used to empty the rail: the folder tree went, and with it the
   only way to see where the drawing sits and what is beside it. The scope of a
   view page is *which view*, so the same tree stays — the same one the Views
   list is narrowed by, drawings and all. It used to be two halves, a tree of
   folders above a flat list of the drawings in the chosen one, which is the
   same eighty-six names twice and a fold to keep in step between them. One
   tree: the folders divide, the drawings hang off them, and the one you are
   reading is marked in it. That is also where the toolbar's picker went — a
   <select> of eighty-six names was a third route to this, and a worse one. */

const scopeHidden = new Set();       // viewpoints the reader has switched off

export function viewsScope(viewId) {
  const spec = specFor("views");
  if (!folded.has(spec.kind)) folded.set(spec.kind, new Set());
  const collapsed = folded.get(spec.kind);

  const under = subtreeCounts(spec.inFolders);
  const roots = store.roots.filter((i) => under.get(i).length);
  const soleRoot = soleRootOf(roots, under, spec.all().length);
  const here = store.byId.get(viewId);
  const at = here && here.kind === "view" ? here.i : null;

  // Open the branch the drawing is in. A rail that has to be dug into before
  // it can say where you are is a rail that says nothing.
  if (at !== null) {
    const path = folderPath(view(at).folder);
    for (let cut = path.length; cut > 0; cut = path.lastIndexOf("/", cut - 1)) {
      collapsed.delete(path.slice(0, cut));
    }
  }

  const treePane = h("div", { class: "rail-scope" });

  // The same dimension the Views list offers, in the same place and the same
  // shape. A drawing's scope is which drawing, and that is narrowed by folder
  // and by viewpoint on both pages or on neither.
  const vpValues = () => tally(store.data.views.map((v) => v.viewpoint || "(none)"));
  const filters = filterBar([{
    key: "viewpoint", label: "Viewpoints", noun: "viewpoints", hidden: scopeHidden,
    values: vpValues,
    onChange: () => drawTree(),
  }]);
  const box = h("div", { class: "rail-split" },
    h("div", { class: "rail-group" }, h("h2", { class: "caps rail-group-title" }, "Filter"), filters),
    h("div", { class: "rail-group rail-group-grow" }, h("h2", { class: "caps rail-group-title" }, "Views"), treePane));

  const passing = () => new Set(store.data.views
    .map((_, i) => i)
    .filter((i) => !scopeHidden.has(view(i).viewpoint || "(none)")));

  function drawTree() {
    const held = treePane.contains(document.activeElement)
      ? document.activeElement.closest(".tree-row")?.dataset.key
      : null;
    const scrolled = treePane.scrollTop;
    clear(treePane);
    treePane.appendChild(tree({
      nodes: folderNodes({ spec, under, roots, soleRoot, collapsed, hits: passing() }),
      selected: at === null ? null : `#${at}`,
      label: "Views",
      isOpen: (k) => !collapsed.has(k),
      onToggle: (k) => { collapsed.has(k) ? collapsed.delete(k) : collapsed.add(k); drawTree(); },
      // A drawing opens on a single click here, as the list it replaces did:
      // this rail is the way from one drawing to the next, not a selection
      // surface. A folder has nothing to narrow any more, so it folds.
      onPick: (k) => {
        if (k.startsWith("#")) { location.hash = href("view", view(+k.slice(1)).id); return; }
        collapsed.has(k) ? collapsed.delete(k) : collapsed.add(k);
        drawTree();
      },
    }));
    treePane.scrollTop = scrolled;
    if (held != null) {
      const row = treePane.querySelector(`.tree-row[data-key="${esc(held)}"]`);
      if (row) { treePane.querySelectorAll(".tree-row").forEach((r) => r.setAttribute("tabindex", "-1")); row.setAttribute("tabindex", "0"); row.focus(); }
    }
    treePane.querySelector(".tree-row.is-selected")?.scrollIntoView({ block: "nearest" });
  }

  drawTree();
  return box;
}

// Where a "back to the list" button should go.
export function lastListParams(kind) {
  return lastLeft.get(kind) || {};
}
