// Views, Elements and Relationships — one page, three configurations.
//
// They used to be three modules with three sortable-table headers (three
// different rules about which columns sort descending first), three
// filter→sort→cap→render pipelines and three ways of filtering: a folder tree
// here, a flat forty-one-entry `<select>` there, a row of chips on the third.
// Nothing about a collection of concepts differs between them except which
// columns to show and which dimensions to offer, so that — and only that — is
// what a spec says below.

import { h, clear, fmt, relLabel } from "../dom.js";
import { store, elem, rel, view, folder, folderPath } from "../store.js";
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
      { key: "name", label: "Name", width: "30%", sortable: true, sort: (i) => name(view(i)), render: (i) => cell(icon("view"), h("span", { class: "ellipsis" }, view(i).name || "(unnamed)")) },
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
    matches: (i, n) => (view(i).name || "").toLowerCase().includes(n),
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
      { key: "type", label: "Kind", width: "14%", sortable: true, sort: (i) => rel(i).type, render: (i) => relLabel(rel(i).type) },
      { key: "source", label: "Source", width: "25%", sortable: true, sort: (i) => endName(rel(i).src), render: (i) => endCell(rel(i).src, rel(i).srcId) },
      { key: "arrow", label: "", width: "3%", cls: "cell-arrow", render: () => icon("arrow-right", { class: "rel-arrow" }) },
      { key: "target", label: "Target", width: "25%", sortable: true, sort: (i) => endName(rel(i).tgt), render: (i) => endCell(rel(i).tgt, rel(i).tgtId) },
      { key: "name", label: "Label", width: "20%", sortable: true, sort: (i) => (rel(i).name || "").toLowerCase(), render: (i) => rel(i).name || detailOf(rel(i)) || h("span", { class: "subtle" }, "—") },
      { key: "views", label: "Views", width: "13%", sortable: true, numeric: true, align: "right", sort: (i) => store.viewsOfRel[i].length, render: (i) => fmt(store.viewsOfRel[i].length) },
    ],
    dimensions: [{
      key: "type", label: "Kinds", noun: "kinds",
      valuesOf: () => tally(store.data.relations.map((r) => r.type), relLabel),
      of: (i) => rel(i).type,
    }],
    defaultSort: "source",
    matches: (i, n) => {
      const r = rel(i);
      return (r.name || "").toLowerCase().includes(n)
        || endName(r.src).includes(n) || endName(r.tgt).includes(n);
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
      { key: "name", label: "Name", width: "29%", sortable: true, sort: (i) => name(elem(i)), render: (i) => cell(typeIcon(elem(i).type), h("span", { class: "ellipsis" }, elem(i).name || "(unnamed)")) },
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
    matches: (i, n) => (elem(i).name || "").toLowerCase().includes(n),
    emptyTitle: "No element matches",
    emptyBody: "Clear the filter, or show more layers and types.",
  };
}

const name = (c) => (c.name || "").toLowerCase();
const endName = (i) => (i >= 0 ? (elem(i).name || "").toLowerCase() : "");
const endCell = (i, raw) => i >= 0
  ? cell(typeIcon(elem(i).type), h("span", { class: "ellipsis" }, elem(i).name || "(unnamed)"))
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
  const count = h("span", { class: "toolbar-meta num" });
  const field = searchField({
    value: state.q, placeholder: `Filter ${spec.noun}…`, width: "var(--ctl-w)",
    oninput: (v) => { state.q = v; push(); redraw(); },
  });
  const bar = toolbar({
    title: spec.title, titleIcon: spec.iconName,
    controls: [field],
    trailing: [count],
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

  if (hasHierarchy && !collapsed.size && under.size > 24) {
    for (const [i, list] of under) if (list.length && folder(i).parent !== null) collapsed.add(folderPath(i));
  }

  function treeNodes(passing) {
    const hits = new Set(passing);
    const count = (i) => under.get(i).reduce((n, x) => n + (hits.has(x) ? 1 : 0), 0);
    const nodes = [{ key: "", label: `All ${spec.noun}`, count: hits.size, depth: 0, hasKids: false, title: `Every one of the ${spec.noun} in the model` }];
    const walk = (i, depth) => {
      const path = folderPath(i);
      const kids = store.folderKids[i].filter((k) => under.get(k).length);
      nodes.push({ key: path, label: folder(i).name, count: count(i), depth, hasKids: kids.length > 0, title: `${folder(i).name}\n${path}` });
      if (!collapsed.has(path)) for (const k of kids) walk(k, depth + 1);
    };
    for (const r of roots) walk(r, 1);
    return nodes;
  }

  function drawTree(passing) {
    clear(treePane);
    if (!hasHierarchy) return;
    treePane.appendChild(tree({
      nodes: treeNodes(passing),
      active: state.folder,
      label: "Folders",
      isOpen: (k) => !collapsed.has(k),
      onToggle: (k) => { collapsed.has(k) ? collapsed.delete(k) : collapsed.add(k); drawTree(passing); },
      onPick: (k) => {
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
  }

  // ---- the table
  const table = dataTable({
    columns: spec.columns.map((c) => ({ ...c, render: (row) => c.render(row.i) })),
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
    const needle = state.q.trim().toLowerCase();
    return spec.all().filter((i) => passesDims(i) && (!needle || spec.matches(i, needle)));
  }

  function redraw() {
    const needle = state.q.trim().toLowerCase();
    const all = spec.all();
    const rows = all.filter((i) => passesDims(i) && inFolder(i) && (!needle || spec.matches(i, needle)));
    const col = spec.columns.find((c) => c.key === state.sort) || spec.columns[0];
    const key = col.sort || ((i) => spec.name(i).toLowerCase());
    rows.sort((a, b) => {
      const ka = key(a), kb = key(b);
      const c = typeof ka === "number" ? ka - kb : String(ka).localeCompare(String(kb), undefined, { numeric: true });
      return state.dir === "asc" ? c : -c;
    });
    table.sort = { key: state.sort, dir: state.dir };
    table.rows = rows.map((i) => ({ i }));
    if (selectedId()) table.setSelected(selectedId(), { reveal: false });
    count.textContent = countLabel(rows.length, all.length);
    drawTree(passingForTree());
  }

  const onSelect = (e) => table.setSelected(e.detail.id);
  document.addEventListener("amcli:select", onSelect);

  redraw();
  return () => document.removeEventListener("amcli:select", onSelect);
}

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

// Where a "back to the list" button should go.
export function lastListParams(kind) {
  return lastLeft.get(kind) || {};
}
