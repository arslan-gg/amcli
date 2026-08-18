// Views: a table of every view, like the element and relationship tables; a
// click opens the drawing full width — the very SVG the server renders from
// the file, so what is on screen is what Archi would draw, and a click on a
// figure opens the concept it stands for.

import { h, clear, fmt } from "../dom.js";
import { store, view, folderPath } from "../store.js";
import { href, replaceParams } from "../router.js";
import { attachPanZoom } from "../panzoom.js";
import { select } from "../app.js";

export function mount(main, route) {
  if (route.page === "view" && route.id) {
    const found = store.byId.get(route.id);
    if (found && found.kind === "view") return renderView(main, found.i, route.params.get("focus"));
    const page = h("div", { class: "page" }, h("div", { class: "page-head" }, h("a", { class: "btn sm", href: href("views") }, "‹ Views")), h("div", { class: "empty" }, `No view has id ${route.id}`));
    main.appendChild(page);
    return () => {};
  }
  return renderTable(main, route);
}

// ---- the table -------------------------------------------------------------

function renderTable(main, route) {
  const page = h("div", { class: "page" });
  const head = h("div", { class: "page-head" });
  const body = h("div", { class: "page-body" });
  page.append(head, body);
  main.appendChild(page);

  const p = route.params;
  const state = { folder: p.get("folder") || "", q: p.get("q") || "", sort: p.get("sort") || "name", dir: p.get("dir") || "asc" };
  const push = () => replaceParams({ folder: state.folder, q: state.q, sort: state.sort === "name" ? "" : state.sort, dir: state.dir === "asc" ? "" : state.dir });

  const all = store.data.views;
  const folderCounts = {};
  for (const v of all) { const f = folderPath(v.folder); folderCounts[f] = (folderCounts[f] || 0) + 1; }

  const chips = h("div", { class: "chips" });
  const q = h("input", { class: "input", type: "search", placeholder: "Filter by name…", value: state.q, style: { width: "220px" },
    oninput: (e) => { state.q = e.target.value; push(); renderRows(); } });
  const summary = h("span", { class: "muted small nowrap" });
  head.append(chips, q, h("span", { class: "spacer" }), summary);
  const table = h("table", { class: "grid" });
  body.append(table);

  const chip = (label, value, count) => h("button", {
    class: "chip" + (state.folder === value ? " active" : ""),
    onclick: () => { state.folder = state.folder === value ? "" : value; push(); renderChips(); renderRows(); },
  }, label, count !== undefined ? h("span", { class: "muted", style: { marginLeft: "5px" } }, count) : null);

  function renderChips() {
    clear(chips);
    chips.appendChild(chip("All folders", "", all.length));
    for (const f of Object.keys(folderCounts).sort()) chips.appendChild(chip(f.replace(/^\/Views\/?/, "") || "/Views", f, folderCounts[f]));
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
    const needle = state.q.trim().toLowerCase();
    const list = [];
    for (let i = 0; i < all.length; i++) {
      const v = all[i];
      if (state.folder && folderPath(v.folder) !== state.folder) continue;
      if (needle && !v.name.toLowerCase().includes(needle)) continue;
      list.push(i);
    }
    const numeric = ["elements", "relations"].includes(state.sort);
    const k = key[state.sort] || key.name;
    list.sort((a, b) => {
      const c = numeric ? k(a) - k(b) : String(k(a)).localeCompare(String(k(b)), undefined, { numeric: true });
      return state.dir === "asc" ? c : -c;
    });
    summary.textContent = `${fmt(list.length)} of ${fmt(all.length)}`;
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
        h("td", { class: "mono muted" }, folderPath(v.folder)),
        h("td", { class: "muted" }, v.viewpoint || ""),
        h("td", { class: "num" }, v.elements.length),
        h("td", { class: "num" }, v.relations.length),
      );
      tr.addEventListener("click", () => { location.hash = href("view", v.id); });
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
  }

  renderChips();
  renderRows();
  return () => {};
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
    const grp = h("optgroup", { label: f });
    for (const i of byFolder.get(f).sort((a, b) => view(a).name.localeCompare(view(b).name, undefined, { numeric: true }))) {
      grp.appendChild(h("option", { value: view(i).id, selected: i === vi }, view(i).name));
    }
    picker.appendChild(grp);
  }

  const head = h("div", { class: "page-head" },
    h("a", { class: "btn sm", href: href("views"), title: "All views" }, "‹ Views"),
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
// outlined. Groups nested inside groups: the innermost wins.
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
