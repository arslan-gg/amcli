// Every relationship as a table: type, source, target, name; filter by type
// and by either end's name.

import { h, clear, fmt, relLabel } from "../dom.js";
import { store, elem, rel } from "../store.js";
import { typeIcon, accessLabel } from "../notation.js";
import { href, replaceParams } from "../router.js";
import { select } from "../app.js";

const CAP = 1000;

export function mount(main, route) {
  const page = h("div", { class: "page" });
  const head = h("div", { class: "page-head" });
  const body = h("div", { class: "page-body" });
  page.append(head, body);
  main.appendChild(page);

  const p = route.params;
  const state = { type: p.get("type") || "", q: p.get("q") || "", sort: p.get("sort") || "type", dir: p.get("dir") || "asc", selected: null };
  const push = () => replaceParams({ type: state.type, q: state.q, sort: state.sort === "type" ? "" : state.sort, dir: state.dir === "asc" ? "" : state.dir });

  const all = store.data.relations;
  const typeCounts = {};
  for (const r of all) typeCounts[r.type] = (typeCounts[r.type] || 0) + 1;

  const chips = h("div", { class: "chips" });
  const q = h("input", { class: "input", type: "search", placeholder: "Filter by name or end…", value: state.q, style: { width: "240px" },
    oninput: (e) => { state.q = e.target.value; push(); renderTable(); } });
  const summary = h("span", { class: "muted small nowrap" });
  head.append(chips, q, h("span", { class: "spacer" }), summary);
  const table = h("table", { class: "grid" });
  const note = h("div", { class: "table-note" });
  body.append(table, note);

  const chip = (label, value, count) => h("button", {
    class: "chip" + (state.type === value ? " active" : ""),
    onclick: () => { state.type = state.type === value ? "" : value; push(); renderChips(); renderTable(); },
  }, label, count !== undefined ? h("span", { class: "muted", style: { marginLeft: "5px" } }, count) : null);

  function renderChips() {
    clear(chips);
    chips.appendChild(chip("All", "", all.length));
    for (const t of Object.keys(typeCounts).sort()) chips.appendChild(chip(t, t, typeCounts[t]));
  }

  const endName = (i) => (i >= 0 ? elem(i).name || "" : "");

  function rows() {
    const needle = state.q.trim().toLowerCase();
    const out = [];
    for (let i = 0; i < all.length; i++) {
      const r = all[i];
      if (state.type && r.type !== state.type) continue;
      if (needle && !((r.name || "").toLowerCase().includes(needle) || endName(r.src).toLowerCase().includes(needle) || endName(r.tgt).toLowerCase().includes(needle))) continue;
      out.push(i);
    }
    const key = {
      type: (i) => rel(i).type,
      source: (i) => endName(rel(i).src).toLowerCase(),
      target: (i) => endName(rel(i).tgt).toLowerCase(),
      name: (i) => (rel(i).name || "").toLowerCase(),
      views: (i) => store.viewsOfRel[i].length,
    }[state.sort] || ((i) => rel(i).type);
    const numeric = state.sort === "views";
    out.sort((a, b) => {
      const ka = key(a), kb = key(b);
      const c = numeric ? ka - kb : String(ka).localeCompare(String(kb)) || endName(rel(a).src).localeCompare(endName(rel(b).src));
      return state.dir === "asc" ? c : -c;
    });
    return out;
  }

  function th(label, k, cls) {
    return h("th", {
      class: (cls || "") + (state.sort === k ? " sorted" + (state.dir === "asc" ? " asc" : "") : ""),
      onclick: () => {
        if (state.sort === k) state.dir = state.dir === "asc" ? "desc" : "asc";
        else { state.sort = k; state.dir = k === "views" ? "desc" : "asc"; }
        push(); renderTable();
      },
    }, label);
  }

  const endCell = (i, rawId) => i >= 0
    ? h("td", null, h("a", { href: href("element", elem(i).id), onclick: (ev) => ev.stopPropagation(), style: { display: "inline-flex", alignItems: "center" } }, typeIcon(elem(i).type), elem(i).name || h("span", { class: "muted" }, "(unnamed)")))
    : h("td", { class: "muted mono small" }, rawId ? `${rawId}` : "—");

  function renderTable() {
    const list = rows();
    summary.textContent = `${fmt(list.length)} of ${fmt(all.length)}`;
    clear(table);
    if (!list.length) {
      table.appendChild(h("tbody", null, h("tr", null, h("td", { colspan: 6 }, h("div", { class: "empty" }, "No relationships match.")))));
      note.textContent = "";
      return;
    }
    table.appendChild(h("thead", null, h("tr", null, th("Type", "type"), th("Source", "source"), th("Target", "target"), th("Name", "name"), h("th", null, "Detail"), th("Views", "views", "num"))));
    const tbody = h("tbody");
    for (const i of list.slice(0, CAP)) {
      const r = rel(i);
      const detail = r.type === "Access" && r.access !== null ? accessLabel(r.access) : r.type === "Association" && r.directed ? "directed" : "";
      const tr = h("tr", { class: state.selected === r.id ? "selected" : "" },
        h("td", { class: "type" }, h("a", { href: href("relation", r.id), onclick: (ev) => ev.stopPropagation() }, relLabel(r.type))),
        endCell(r.src, r.srcId),
        endCell(r.tgt, r.tgtId),
        h("td", null, r.name || ""),
        h("td", { class: "muted small" }, detail),
        h("td", { class: "num" }, store.viewsOfRel[i].length),
      );
      tr.addEventListener("click", () => {
        tbody.querySelector("tr.selected")?.classList.remove("selected");
        tr.classList.add("selected");
        state.selected = r.id;
        select(r.id);
      });
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    note.textContent = list.length > CAP ? `Showing the first ${fmt(CAP)} of ${fmt(list.length)} — narrow the filter to see the rest.` : "";
  }

  renderChips();
  renderTable();
  return () => {};
}
