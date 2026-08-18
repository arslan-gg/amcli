// Every element in the model as a table: filter by layer, type, folder and
// name; sort by any column; click a row for the details.

import { h, clear, fmt } from "../dom.js";
import { store, elem, folderPath } from "../store.js";
import { typeIcon, typeOf } from "../notation.js";
import { href, replaceParams } from "../router.js";
import { select } from "../app.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];
const LAYER_LABEL = {};
const CAP = 1000;

export function mount(main, route) {
  const page = h("div", { class: "page" });
  const head = h("div", { class: "page-head" });
  const body = h("div", { class: "page-body" });
  page.append(head, body);
  main.appendChild(page);

  const p = route.params;
  const state = {
    layer: p.get("layer") || "",
    type: p.get("type") || "",
    folder: p.get("folder") || "",
    q: p.get("q") || "",
    sort: p.get("sort") || "name",
    dir: p.get("dir") || "asc",
    selected: null,
  };
  const push = () => replaceParams({ layer: state.layer, type: state.type, folder: state.folder, q: state.q, sort: state.sort === "name" ? "" : state.sort, dir: state.dir === "asc" ? "" : state.dir });

  const all = store.data.elements;
  const layerCounts = {}, typeCounts = {};
  for (const e of all) {
    layerCounts[e.layer] = (layerCounts[e.layer] || 0) + 1;
    typeCounts[e.type] = (typeCounts[e.type] || 0) + 1;
  }

  // ---- controls ---------------------------------------------------------
  const chips = h("div", { class: "chips" });
  const chip = (label, value, count) => h("button", {
    class: "chip" + (state.layer === value ? " active" : ""),
    onclick: () => { state.layer = state.layer === value ? "" : value; state.type = ""; push(); render(); },
  }, label, count !== undefined ? h("span", { class: "muted", style: { marginLeft: "5px" } }, count) : null);
  const typeSel = h("select", { class: "input", onchange: (e) => { state.type = e.target.value; push(); render(); } });
  const folderSel = h("select", { class: "input", onchange: (e) => { state.folder = e.target.value; push(); render(); } });
  const q = h("input", { class: "input", type: "search", placeholder: "Filter by name…", value: state.q, style: { width: "220px" },
    oninput: (e) => { state.q = e.target.value; push(); renderTable(); } });
  const summary = h("span", { class: "muted small nowrap" });
  head.append(chips, typeSel, folderSel, q, h("span", { class: "spacer" }), summary);

  const table = h("table", { class: "grid" });
  const note = h("div", { class: "table-note" });
  body.append(table, note);

  function render() {
    clear(chips);
    chips.appendChild(chip("All layers", "", all.length));
    for (const l of LAYERS) if (layerCounts[l]) chips.appendChild(chip(LAYER_LABEL[l] || l, l, layerCounts[l]));
    clear(typeSel);
    typeSel.appendChild(h("option", { value: "" }, "All types"));
    const types = Object.keys(typeCounts).filter((t) => !state.layer || typeOf(t).layer === state.layer).sort();
    for (const t of types) typeSel.appendChild(h("option", { value: t, selected: state.type === t }, `${t} (${typeCounts[t]})`));
    if (state.type && !types.includes(state.type)) state.type = "";
    clear(folderSel);
    folderSel.appendChild(h("option", { value: "" }, "All folders"));
    const paths = [...new Set(store.data.folders.map((f) => f.path))].filter((pth) => pth && !pth.startsWith("/Views") && !pth.startsWith("/Relations")).sort();
    for (const pth of paths) folderSel.appendChild(h("option", { value: pth, selected: state.folder === pth }, pth));
    renderTable();
  }

  function rows() {
    const needle = state.q.trim().toLowerCase();
    const out = [];
    for (let i = 0; i < all.length; i++) {
      const e = all[i];
      if (state.layer && e.layer !== state.layer) continue;
      if (state.type && e.type !== state.type) continue;
      if (state.folder && !(folderPath(e.folder) === state.folder || folderPath(e.folder).startsWith(state.folder + "/"))) continue;
      if (needle && !(e.name || "").toLowerCase().includes(needle)) continue;
      out.push(i);
    }
    const key = {
      name: (i) => (elem(i).name || "").toLowerCase(),
      type: (i) => elem(i).type,
      layer: (i) => elem(i).layer,
      folder: (i) => folderPath(elem(i).folder),
      in: (i) => store.inc[i].length,
      out: (i) => store.out[i].length,
      views: (i) => store.viewsOfElem[i].length,
    }[state.sort] || ((i) => (elem(i).name || "").toLowerCase());
    const numeric = ["in", "out", "views"].includes(state.sort);
    out.sort((a, b) => {
      const ka = key(a), kb = key(b);
      const c = numeric ? ka - kb : String(ka).localeCompare(String(kb));
      return state.dir === "asc" ? c : -c;
    });
    return out;
  }

  function th(label, k, cls) {
    return h("th", {
      class: (cls || "") + (state.sort === k ? " sorted" + (state.dir === "asc" ? " asc" : "") : ""),
      onclick: () => {
        if (state.sort === k) state.dir = state.dir === "asc" ? "desc" : "asc";
        else { state.sort = k; state.dir = ["in", "out", "views"].includes(k) ? "desc" : "asc"; }
        push(); renderTable();
      },
    }, label);
  }

  function renderTable() {
    const list = rows();
    summary.textContent = `${fmt(list.length)} of ${fmt(all.length)}`;
    clear(table);
    if (!list.length) {
      table.appendChild(h("tbody", null, h("tr", null, h("td", { colspan: 7 }, h("div", { class: "empty" }, "No elements match.")))));
      note.textContent = "";
      return;
    }
    table.appendChild(h("thead", null, h("tr", null,
      th("Name", "name"), th("Type", "type"), th("Layer", "layer"), th("Folder", "folder"),
      th("In", "in", "num"), th("Out", "out", "num"), th("Views", "views", "num"))));
    const tbody = h("tbody");
    const shown = list.slice(0, CAP);
    for (const i of shown) {
      const e = elem(i);
      const t = typeOf(e.type);
      const tr = h("tr", { class: state.selected === e.id ? "selected" : "", dataset: { id: e.id } },
        h("td", null, h("a", { href: href("element", e.id), onclick: (ev) => ev.stopPropagation() }, e.name || h("span", { class: "muted" }, "(unnamed)"))),
        h("td", { class: "type" }, h("span", { class: "swatch", style: { background: t.fill } }), typeIcon(e.type), e.type),
        h("td", null, LAYER_LABEL[e.layer] || e.layer),
        h("td", { class: "mono muted" }, folderPath(e.folder)),
        h("td", { class: "num" }, store.inc[i].length),
        h("td", { class: "num" }, store.out[i].length),
        h("td", { class: "num" }, store.viewsOfElem[i].length),
      );
      tr.addEventListener("click", () => {
        tbody.querySelector("tr.selected")?.classList.remove("selected");
        tr.classList.add("selected");
        state.selected = e.id;
        select(e.id);
      });
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    note.textContent = list.length > CAP ? `Showing the first ${fmt(CAP)} of ${fmt(list.length)} — narrow the filter to see the rest.` : "";
  }

  render();
  return () => {};
}
