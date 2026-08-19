// The graph: a view of the model that was never saved.
//
// The page decides *what* to draw — it holds the whole model already, so a
// chip toggles on the keystroke — and asks the server *where*, because the
// answer is `amcli-view`'s layout, the very code `view auto` runs. So the
// picture here is the picture `amcli view auto --from <centre> -n <depth>`
// would file in the model: same rows, same box sizes, same straight lines.
// A force simulation drew something else, and on a real model it drew a
// hairball — two hundred and seventy boxes in a disc, every label unreadable.
//
// The drawing is not draggable, for the same reason a view is not: where a
// box goes is the layout's answer, and moving one by hand only makes the
// picture disagree with what amcli would draw. What *is* live is the top
// menu — centre, depth, direction, layers, types, relationships — and a
// double-click on a box, which re-centres the graph there.

import { h, s, clear, fmt, relLabel } from "../dom.js";
import { store, elem, rel, search } from "../store.js";
import { ensureSymbols, nodeGroup, relStyle, typeOf, typeIcon } from "../notation.js";
import { replaceParams } from "../router.js";
import { attachPanZoom } from "../panzoom.js";
import { select } from "../app.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];
const LAYER_LABEL = { "Implementation & Migration": "Impl. & Migration" };
const WARN_AT = 400;  // ask before drawing more than this
const HARD_CAP = 600; // what the server will lay out, and what a person can read

export function mount(main, route) {
  const p = route.params;
  const state = {
    focus: p.get("focus") || "",
    depth: clamp(parseInt(p.get("depth") || "2", 10) || 2, 1, 4),
    dir: ["in", "out", "both"].includes(p.get("dir")) ? p.get("dir") : "both",
    hideLayers: new Set((p.get("hide") || "").split(",").filter(Boolean)),
    hideTypes: new Set((p.get("hidetype") || "").split(",").filter(Boolean)),
    hideRels: new Set((p.get("hiderel") || "").split(",").filter(Boolean)),
    all: p.get("all") === "1",
    pinned: new Set((p.get("pin") || "").split(",").filter(Boolean)),
    algo: ["auto", "grid"].includes(p.get("algo")) ? p.get("algo") : "auto",
    confirmed: false,
    selected: null,
  };
  const push = () => replaceParams({
    focus: state.focus, depth: state.depth === 2 ? "" : String(state.depth), dir: state.dir === "both" ? "" : state.dir,
    hide: [...state.hideLayers].join(","), hidetype: [...state.hideTypes].join(","),
    hiderel: [...state.hideRels].join(","), all: state.all ? "1" : "",
    algo: state.algo === "auto" ? "" : state.algo, pin: [...state.pinned].join(","),
  });

  const page = h("div", { class: "page" });
  const head = h("div", { class: "page-head" });
  const filters = h("div", { class: "filter-bar" });
  const canvas = h("div", { class: "canvas" });
  page.append(head, filters, canvas);
  main.appendChild(page);

  // ---- what the graph is centred on --------------------------------------
  // The centre is shown as a chip, not as text in the search box. Typing a
  // name is how you *look for* a new centre; it is not a record of the one
  // you have, and leaving the old name in the box made every re-centring
  // start by clearing someone else's text.
  const centre = h("span", { class: "centre" });
  const focusBox = h("div", { class: "search-box", style: { width: "220px" } });
  const focusInput = h("input", { class: "input", type: "search", placeholder: "Centre on an element…", style: { width: "100%", paddingRight: "8px" }, autocomplete: "off" });
  const focusResults = h("div", { class: "search-results", hidden: true, style: { top: "32px" } });
  focusBox.append(focusInput, focusResults);

  function renderCentre() {
    clear(centre);
    const f = state.focus ? store.byId.get(state.focus) : null;
    if (f && f.kind === "element") {
      const e = elem(f.i);
      centre.append(h("span", { class: "badge solid", title: `Centred on ${e.name}` },
        typeIcon(e.type), h("span", { class: "ellipsis", style: { maxWidth: "220px" } }, e.name),
        h("button", { class: "x", title: "Stop centring here", onclick: () => { state.focus = ""; state.confirmed = false; push(); renderCentre(); build(); } }, "✕")));
    } else if (!state.all) {
      centre.append(h("span", { class: "muted small nowrap" }, "Nothing centred"));
    }
  }

  // A new centre replaces whatever was typed to find it, so the box is ready
  // for the next search rather than holding the last one.
  function recentre(id) {
    state.focus = id;
    state.all = false;
    state.confirmed = false;
    focusInput.value = "";
    focusResults.hidden = true;
    allBtn.classList.remove("active");
    push();
    renderCentre();
    build();
  }

  focusInput.addEventListener("input", () => {
    const hits = search(focusInput.value, 12).elements;
    clear(focusResults);
    if (!hits.length || !focusInput.value.trim()) { focusResults.hidden = true; return; }
    for (const { i } of hits) {
      const e = elem(i);
      focusResults.appendChild(h("a", { href: "#", onclick: (ev) => { ev.preventDefault(); recentre(e.id); } },
        typeIcon(e.type), h("span", { class: "ellipsis" }, e.name), h("span", { class: "type" }, e.type)));
    }
    focusResults.hidden = false;
  });
  focusInput.addEventListener("keydown", (ev) => { if (ev.key === "Escape") { focusResults.hidden = true; } if (ev.key === "Enter") { focusResults.querySelector("a")?.click(); } });
  focusInput.addEventListener("blur", () => setTimeout(() => (focusResults.hidden = true), 150));

  const depthSel = h("select", { class: "input", onchange: (e) => { state.depth = +e.target.value; state.confirmed = false; push(); build(); } },
    [1, 2, 3, 4].map((n) => h("option", { value: n, selected: state.depth === n }, `depth ${n}`)));
  const dirGroup = h("div", { class: "btn-group" }, ["out", "both", "in"].map((d) =>
    h("button", { class: "btn sm" + (state.dir === d ? " active" : ""), dataset: { dir: d }, onclick: () => { state.dir = d; state.confirmed = false; push(); dirGroup.querySelectorAll(".btn").forEach((b) => b.classList.toggle("active", b.dataset.dir === d)); build(); } }, d === "both" ? "↔" : d === "out" ? "→ out" : "← in")));
  const allBtn = h("button", { class: "btn sm" + (state.all ? " active" : ""), onclick: () => { state.all = !state.all; state.confirmed = false; allBtn.classList.toggle("active", state.all); push(); renderCentre(); build(); } }, "Whole model");
  // Two, not three: `layered` and `auto` differ only when the layering comes
  // out so wide that the grid is worth taking instead, and with the graph's
  // lanes free it almost never does — so offering both put the same drawing
  // on the menu twice. What actually ran is on the count, beside the edges.
  const algoSel = h("select", { class: "input", title: "How to place the boxes", onchange: (e) => { state.algo = e.target.value; push(); build(); } },
    [["auto", "auto layout"], ["grid", "grid: by name, ignoring the lines"]].map(([v, l]) => h("option", { value: v, selected: state.algo === v }, l)));
  const countLabel = h("span", { class: "muted small nowrap" });
  head.append(centre, focusBox, depthSel, dirGroup, allBtn, algoSel, h("span", { class: "spacer" }), countLabel);

  // ---- filters ------------------------------------------------------------
  // One labelled row each — they used to share a line, and the word
  // "Relations" ended up alone at the far right of the layers with its chips
  // wrapped underneath it, labelling nothing.
  //
  // Each row is a count and a menu, not a chip per value: this model has
  // thirty-one element types and they wrapped over three lines of the top
  // menu, which is not a filter anyone reads. Beside the menu is what is
  // *hidden*, because that is the short list and the one worth a click.
  const layerRow = h("div", { class: "filter-row" });
  const typeRow = h("div", { class: "filter-row" });
  const relRow = h("div", { class: "filter-row" });
  const pinRow = h("div", { class: "filter-row" });
  filters.append(
    h("span", { class: "k" }, "Layers"), layerRow,
    h("span", { class: "k" }, "Types"), typeRow,
    h("span", { class: "k" }, "Relations"), relRow,
    h("span", { class: "k" }, "Pinned"), pinRow,
  );

  // A pinned element is on the graph whatever else the menu says: outside the
  // depth, in a hidden layer, of a hidden type. It is how you keep the one
  // box you are actually asking about in view while you narrow everything
  // around it.
  function pin(id, on) {
    if (on) state.pinned.add(id); else state.pinned.delete(id);
    push();
    renderPins();
    build();
  }
  function renderPins() {
    clear(pinRow);
    const live = [...state.pinned].filter((id) => store.byId.get(id)?.kind === "element");
    if (!live.length) {
      pinRow.appendChild(h("span", { class: "muted small" }, "none — shift-click a box to keep it on the graph"));
      return;
    }
    for (const id of live) {
      const e = elem(store.byId.get(id).i);
      pinRow.appendChild(h("button", { class: "chip", title: `Unpin ${e.name}`, onclick: () => pin(id, false) },
        typeIcon(e.type), e.name, " ✕"));
    }
    if (live.length > 1) pinRow.appendChild(h("button", { class: "chip", onclick: () => { state.pinned.clear(); push(); renderPins(); build(); } }, "unpin all"));
  }

  const presentLayers = LAYERS.filter((l) => store.data.elements.some((e) => e.layer === l));
  const presentRels = [...new Set(store.data.relations.map((r) => r.type))].sort();
  const typeCounts = {}, layerCounts = {}, relCounts = {};
  for (const e of store.data.elements) {
    typeCounts[e.type] = (typeCounts[e.type] || 0) + 1;
    layerCounts[e.layer] = (layerCounts[e.layer] || 0) + 1;
  }
  for (const r of store.data.relations) relCounts[r.type] = (relCounts[r.type] || 0) + 1;

  // Types are offered for the layers still on show: a list of thirty is long
  // enough without the ones that could not appear anyway.
  const shownTypes = () => Object.keys(typeCounts).filter((t) => !state.hideLayers.has(typeOf(t).layer)).sort();

  // One row: the menu button, then a chip per hidden value so putting one
  // back is a single click.
  function filterRow(where, { keys, hidden, noun, label, fill, count, after }) {
    const redraw = () => { state.confirmed = false; push(); render(); after(); };
    const menu = h("div", { class: "filter-menu", hidden: true });
    const btn = h("button", { class: "btn sm", onclick: (ev) => { ev.stopPropagation(); menu.hidden ? open() : close(); } });
    const ctl = h("div", { class: "filter-ctl" }, btn, menu);

    const onOutside = (ev) => { if (!ctl.contains(ev.target)) close(); };
    const onKey = (ev) => { if (ev.key === "Escape") close(); };
    function close() { menu.hidden = true; document.removeEventListener("pointerdown", onOutside); document.removeEventListener("keydown", onKey); }
    function open() {
      fillMenu();
      menu.hidden = false;
      document.addEventListener("pointerdown", onOutside);
      document.addEventListener("keydown", onKey);
    }
    closers.push(close);

    function fillMenu() {
      clear(menu);
      const ks = keys();
      menu.appendChild(h("div", { class: "head" },
        h("button", { class: "chip", onclick: () => { for (const k of ks) hidden.delete(k); redraw(); fillMenu(); } }, "All"),
        h("button", { class: "chip", onclick: () => { for (const k of ks) hidden.add(k); redraw(); fillMenu(); } }, "None"),
        h("span", { class: "spacer" }),
        h("span", { class: "muted small" }, `${fmt(ks.length - ks.filter((k) => hidden.has(k)).length)} of ${fmt(ks.length)}`)));
      for (const k of ks) {
        const box = h("input", { type: "checkbox", checked: !hidden.has(k),
          onchange: (ev) => { if (ev.target.checked) hidden.delete(k); else hidden.add(k); redraw(); fillMenu(); } });
        menu.appendChild(h("label", { title: label(k) },
          box,
          fill(k) ? h("span", { class: "swatch", style: { background: fill(k) } }) : null,
          h("span", { class: "ellipsis" }, label(k)),
          h("span", { class: "n" }, fmt(count(k)))));
      }
    }

    function render() {
      clear(where);
      const ks = keys();
      const off = ks.filter((k) => hidden.has(k));
      btn.textContent = off.length ? `${fmt(ks.length - off.length)} of ${fmt(ks.length)} ${noun} ▾` : `all ${fmt(ks.length)} ${noun} ▾`;
      btn.classList.toggle("active", off.length > 0);
      where.append(ctl);
      if (off.length) {
        where.appendChild(h("span", { class: "muted small" }, "hiding"));
        for (const k of off) {
          where.appendChild(h("button", { class: "chip off", title: `Show ${label(k)} again`,
            onclick: () => { hidden.delete(k); redraw(); } }, label(k), " ✕"));
        }
      }
      if (!menu.hidden) fillMenu();
    }
    return render;
  }

  const closers = [];
  const drawLayers = filterRow(layerRow, {
    keys: () => presentLayers, hidden: state.hideLayers, noun: "layers",
    label: (l) => LAYER_LABEL[l] || l, fill: (l) => layerFill(l), count: (l) => layerCounts[l] || 0,
    // Hiding a layer takes its types off the type menu, so that has to be
    // redrawn with it.
    after: () => { drawTypes(); build(); },
  });
  const drawTypes = filterRow(typeRow, {
    keys: shownTypes, hidden: state.hideTypes, noun: "types",
    label: (t) => t, fill: (t) => typeOf(t).fill, count: (t) => typeCounts[t] || 0,
    after: build,
  });
  const drawRels = filterRow(relRow, {
    keys: () => presentRels, hidden: state.hideRels, noun: "kinds",
    label: (t) => relLabel(t), fill: () => null, count: (t) => relCounts[t] || 0,
    after: build,
  });
  drawLayers(); drawTypes(); drawRels();

  // ---- canvas -----------------------------------------------------------
  const svg = s("svg", { class: "graph" });
  ensureSymbols(svg);
  const gEdges = s("g", { class: "edges" });
  const gNodes = s("g", { class: "nodes" });
  svg.append(gEdges, gNodes);
  const hud = h("div", { class: "canvas-hud" });
  const legend = h("div", { class: "legend" });
  const msg = h("div", { class: "canvas-msg" });
  canvas.append(svg, hud, legend, msg);
  // The floor stops a drawing far wider than the pane from being shrunk
  // until every box is a fraction of a pixel and the canvas looks empty.
  const pz = attachPanZoom(svg, canvas, { maxFitScale: 1.25, minFitScale: 0.14 });
  pz.fit({ x: -400, y: -300, w: 800, h: 600 });
  hud.append(
    h("button", { class: "btn sm", onclick: () => fitAll(), title: "Fit" }, "Fit"),
    h("button", { class: "btn sm", onclick: () => pz.zoomIn() }, "+"),
    h("button", { class: "btn sm", onclick: () => pz.zoomOut() }, "−"),
  );

  let nodes = [], links = [];
  // Only the newest request may draw: a chip toggled while one is in flight
  // must not be overtaken by the answer to the question before it.
  let generation = 0;
  let alive = true;

  function build() {
    const set = chooseNodes();
    if (set === null) return; // a message was shown instead
    const want = [...set].sort((a, b) => a - b);
    const mine = ++generation;
    showMessage("Laying out…");
    const url = `/api/layout?e=${encodeURIComponent(compact(want))}`
      + `&hiderel=${encodeURIComponent([...state.hideRels].join(","))}`
      + `&algo=${encodeURIComponent(state.algo)}`
      + `&c=${encodeURIComponent(store.checksum)}`;
    fetch(url, { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : r.json().then((e) => Promise.reject(new Error(e.error || `HTTP ${r.status}`)))))
      .then((placed) => {
        if (!alive || mine !== generation) return;
        msg.hidden = true;
        draw(want, placed);
      })
      .catch((e) => {
        if (!alive || mine !== generation) return;
        showMessage(`Could not lay this out: ${e.message}`);
      });
  }

  // Which elements to show. `null` means a message was shown instead.
  function chooseNodes() {
    msg.hidden = true;
    const shown = (i) => !state.hideLayers.has(elem(i).layer) && !state.hideTypes.has(elem(i).type);
    const passRel = (r) => !state.hideRels.has(r.type);
    let set;
    if (state.all) {
      set = new Set();
      store.data.elements.forEach((_, i) => { if (shown(i)) set.add(i); });
    } else if (state.focus) {
      const f = store.byId.get(state.focus);
      if (!f || f.kind !== "element") { showMessage(`No element has id ${state.focus}.`); return null; }
      if (!shown(f.i)) { showMessage("The centre is in a layer or of a type this graph is hiding."); return null; }
      set = new Set([f.i]);
      let frontier = [f.i];
      for (let d = 0; d < state.depth && frontier.length; d++) {
        const next = [];
        for (const i of frontier) {
          const arcs = [];
          if (state.dir !== "in") for (const ri of store.out[i]) arcs.push([ri, rel(ri).tgt]);
          if (state.dir !== "out") for (const ri of store.inc[i]) arcs.push([ri, rel(ri).src]);
          for (const [ri, o] of arcs) {
            if (o < 0 || set.has(o) || !passRel(rel(ri)) || !shown(o)) continue;
            set.add(o); next.push(o);
          }
        }
        frontier = next;
      }
    } else {
      set = new Set();
    }
    // Whatever the filters said, a pinned element is on the graph.
    for (const id of state.pinned) {
      const f = store.byId.get(id);
      if (f && f.kind === "element") set.add(f.i);
    }
    if (set.size === 0) {
      clear(gEdges); clear(gNodes); nodes = []; links = [];
      const top = store.data.elements.map((_, i) => [i, store.out[i].length + store.inc[i].length]).sort((a, b) => b[1] - a[1]).slice(0, 8);
      showMessage(h("div", null,
        h("p", null, "Centre the graph on an element, or draw the whole model."),
        top.length ? h("div", { class: "link-list", style: { marginTop: "12px", textAlign: "left" } },
          h("div", { class: "muted small", style: { padding: "0 6px 4px" } }, "Most connected"),
          top.map(([i, deg]) => h("a", { href: "#", onclick: (ev) => { ev.preventDefault(); recentre(elem(i).id); } }, typeIcon(elem(i).type), h("span", { class: "ellipsis" }, elem(i).name), h("span", { class: "muted small", style: { marginLeft: "auto" } }, `${deg}`)))) : null,
      ));
      return null;
    }
    if (set.size > HARD_CAP) {
      showMessage(`That is ${fmt(set.size)} elements — more than the ${fmt(HARD_CAP)} this graph will draw. Hide a layer or a type, lower the depth, or centre on an element.`);
      return null;
    }
    if (set.size > WARN_AT && !state.confirmed) {
      showMessage(h("div", null,
        h("p", null, `${fmt(set.size)} elements is a lot to read at once.`),
        h("div", { class: "actions", style: { justifyContent: "center", marginTop: "10px" } },
          h("button", { class: "btn sm primary", onclick: () => { state.confirmed = true; build(); } }, "Draw it anyway"),
          h("button", { class: "btn sm", onclick: () => { state.all = false; allBtn.classList.remove("active"); push(); renderCentre(); build(); } }, "Never mind")),
      ));
      return null;
    }
    return set;
  }

  function showMessage(content) {
    clear(msg);
    msg.append(typeof content === "string" ? document.createTextNode(content) : content);
    msg.hidden = false;
  }

  // ---- drawing ----------------------------------------------------------
  // `want` is the element indices in the order they were sent; `placed.nodes`
  // is one rectangle each in that same order, and `placed.edges` the
  // relationships the server found between them.
  function draw(want, placed) {
    clear(gEdges); clear(gNodes);
    nodes = want.map((i, at) => {
      const [x, y, w, h] = placed.nodes[at];
      const e = elem(i);
      return { i, id: e.id, type: e.type, name: e.name, x, y, w, h };
    });
    links = [];
    for (const [ri, a, b] of placed.edges) {
      if (a === b) continue; // a relationship onto itself has no line to draw
      links.push({ ri, r: rel(ri), source: nodes[a], target: nodes[b] });
    }
    const box = extent();
    countLabel.textContent = `${fmt(nodes.length)} nodes · ${fmt(links.length)} edges · ${placed.algorithm} · ${fmt(box.w)}×${fmt(box.h)}`;
    countLabel.title = `Laid out by amcli-view, the same code \`view auto\` runs.`;

    for (const l of links) {
      const st = relStyle(l.r);
      const g = s("g", { class: "edge", dataset: { rel: l.r.id } });
      const line = s("line", { "stroke-dasharray": st.dash || null,
        "marker-end": st.target !== "none" ? `url(#m-${st.target}-end)` : null,
        "marker-start": st.source !== "none" ? `url(#m-${st.source}-start)` : null });
      const hit = s("line", { class: "hit" });
      g.append(line, hit);
      if (l.r.name) g.appendChild(s("text", { "text-anchor": "middle" }, l.r.name));
      g.addEventListener("click", (ev) => { ev.stopPropagation(); if (canvas.dataset.justDragged) return; setSelected(l.r.id, g); select(l.r.id); });
      l.el = g; l.line = line; l.hit = hit; l.label = g.querySelector("text");
      gEdges.appendChild(g);
    }
    for (const n of nodes) {
      const g = nodeGroup(n.type, n.name, n.w, n.h);
      g.dataset.id = n.id;
      g.setAttribute("transform", `translate(${n.x},${n.y})`);
      if (n.id === state.focus) g.classList.add("focus");
      if (state.pinned.has(n.id)) g.classList.add("pinned");
      if (state.selected === n.id) g.classList.add("selected");
      const how = state.pinned.has(n.id) ? "shift-click to unpin" : "double-click to centre the graph here · shift-click to pin";
      g.appendChild(s("title", null, `${n.name}\n${n.type} — ${how}`));
      g.addEventListener("click", (ev) => {
        ev.stopPropagation();
        if (canvas.dataset.justDragged) return;
        if (ev.shiftKey) { pin(n.id, !state.pinned.has(n.id)); return; }
        setSelected(n.id, g); select(n.id);
      });
      g.addEventListener("dblclick", (ev) => { ev.stopPropagation(); ev.preventDefault(); recentre(n.id); });
      n.el = g;
      gNodes.appendChild(g);
    }
    position();
    fitAll();
    renderLegend();
  }

  // Every line runs centre to centre and is clipped to the two outlines, the
  // way the renderer draws a connection with no bendpoints.
  function position() {
    for (const l of links) {
      const a = centreOf(l.source), b = centreOf(l.target);
      const p1 = clipToRect(a, b, l.source);
      const p2 = clipToRect(b, a, l.target);
      for (const ln of [l.line, l.hit]) { ln.setAttribute("x1", p1.x.toFixed(1)); ln.setAttribute("y1", p1.y.toFixed(1)); ln.setAttribute("x2", p2.x.toFixed(1)); ln.setAttribute("y2", p2.y.toFixed(1)); }
      if (l.label) { l.label.setAttribute("x", ((p1.x + p2.x) / 2).toFixed(1)); l.label.setAttribute("y", ((p1.y + p2.y) / 2 - 4).toFixed(1)); }
    }
  }

  function centreOf(n) { return { x: n.x + n.w / 2, y: n.y + n.h / 2 }; }

  // Where the ray from `from` toward `to` leaves the box `n`.
  function clipToRect(from, to, n) {
    const dx = to.x - from.x, dy = to.y - from.y;
    if (dx === 0 && dy === 0) return { ...from };
    const c = centreOf(n);
    const sx = from.x - c.x, sy = from.y - c.y;
    let t = Infinity;
    if (dx !== 0) { const tt = ((dx > 0 ? n.w / 2 : -n.w / 2) - sx) / dx; if (tt > 0) t = Math.min(t, tt); }
    if (dy !== 0) { const tt = ((dy > 0 ? n.h / 2 : -n.h / 2) - sy) / dy; if (tt > 0) t = Math.min(t, tt); }
    if (!isFinite(t)) t = 0;
    return { x: from.x + dx * t, y: from.y + dy * t };
  }

  function setSelected(id, el) {
    state.selected = id;
    svg.querySelectorAll(".selected").forEach((x) => x.classList.remove("selected"));
    el?.classList.add("selected");
    // The neighbourhood of a selected node stands out; the rest fades.
    const n = nodes.find((x) => x.id === id);
    svg.querySelectorAll(".dim").forEach((x) => x.classList.remove("dim"));
    if (n) {
      const keep = new Set([n]);
      for (const l of links) { if (l.source === n) keep.add(l.target); if (l.target === n) keep.add(l.source); }
      for (const m of nodes) if (!keep.has(m)) m.el.classList.add("dim");
      for (const l of links) if (l.source !== n && l.target !== n) l.el.classList.add("dim");
    }
  }
  svg.addEventListener("click", () => { if (canvas.dataset.justDragged) return; state.selected = null; svg.querySelectorAll(".selected, .dim").forEach((x) => { x.classList.remove("selected"); x.classList.remove("dim"); }); });

  function extent() {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const n of nodes) { x0 = Math.min(x0, n.x); y0 = Math.min(y0, n.y); x1 = Math.max(x1, n.x + n.w); y1 = Math.max(y1, n.y + n.h); }
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  }

  function fitAll() {
    if (!nodes.length) return;
    pz.fit(extent(), 40);
  }

  function renderLegend() {
    clear(legend);
    const present = new Map();
    for (const n of nodes) present.set(typeOf(n.type).layer, layerFill(typeOf(n.type).layer));
    for (const l of LAYERS) if (present.has(l)) legend.append(h("span", null, h("span", { class: "swatch", style: { background: present.get(l) } }), LAYER_LABEL[l] || l));
    legend.hidden = present.size === 0;
  }

  renderCentre();
  renderPins();
  build();
  // The menus listen on the document while they are open, so leaving the
  // page has to shut them.
  return () => { alive = false; for (const c of closers) c(); pz.destroy(); };
}

// Indices as ranges — `0-271`, `3,7-9,12` — so the whole model fits in a
// query string instead of a kilobyte of commas.
function compact(sorted) {
  const out = [];
  let i = 0;
  while (i < sorted.length) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
    out.push(i === j ? `${sorted[i]}` : `${sorted[i]}-${sorted[j]}`);
    i = j + 1;
  }
  return out.join(",");
}

function layerFill(layer) {
  for (const t of Object.values(store.data.types)) if (t.layer === layer) return t.fill;
  return "#ffffff";
}

function clamp(v, lo, hi) { return Math.max(lo, Math.min(hi, v)); }
