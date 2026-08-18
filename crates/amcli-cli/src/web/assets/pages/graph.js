// The graph: elements as ArchiMate figures, relationships as the lines Archi
// would draw between them, laid out by a force simulation and open to
// exploration — start from one element and a depth, or from the whole model,
// filter by layer and relationship type, double-click to pull in a node's
// neighbours, drag to arrange, click for the details.

import { h, s, clear, fmt, relLabel } from "../dom.js";
import { store, elem, rel, search, otherEnd } from "../store.js";
import { ensureSymbols, nodeGroup, relStyle, typeOf, typeIcon, wrap, PER_CHAR } from "../notation.js";
import { replaceParams } from "../router.js";
import { attachPanZoom } from "../panzoom.js";
import { createSimulation } from "../force.js";
import { select } from "../app.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];
const LAYER_LABEL = { "Implementation & Migration": "Impl. & Migration" };
const WARN_AT = 400;   // ask before drawing more than this
const HARD_CAP = 1500; // never draw more than this
const NODE_H = 44;

export function mount(main, route) {
  const p = route.params;
  const state = {
    focus: p.get("focus") || "",
    depth: clamp(parseInt(p.get("depth") || "2", 10) || 2, 1, 4),
    dir: ["in", "out", "both"].includes(p.get("dir")) ? p.get("dir") : "both",
    hideLayers: new Set((p.get("hide") || "").split(",").filter(Boolean)),
    hideRels: new Set((p.get("hiderel") || "").split(",").filter(Boolean)),
    all: p.get("all") === "1",
    confirmed: false,
    expanded: new Set(),   // element indices whose neighbours were pulled in
    selected: null,
  };
  const push = () => replaceParams({
    focus: state.focus, depth: state.depth === 2 ? "" : String(state.depth), dir: state.dir === "both" ? "" : state.dir,
    hide: [...state.hideLayers].join(","), hiderel: [...state.hideRels].join(","), all: state.all ? "1" : "",
  });

  const page = h("div", { class: "page" });
  const head = h("div", { class: "page-head" });
  const filters = h("div", { class: "page-head", style: { minHeight: "0", padding: "6px 16px" } });
  const canvas = h("div", { class: "canvas" });
  page.append(head, filters, canvas);
  main.appendChild(page);

  // ---- controls ---------------------------------------------------------
  const focusBox = h("div", { class: "search-box", style: { width: "260px" } });
  const focusInput = h("input", { class: "input", type: "search", placeholder: "Start from an element…", style: { width: "100%", paddingRight: "8px" }, autocomplete: "off" });
  const focusResults = h("div", { class: "search-results", hidden: true, style: { top: "32px" } });
  focusBox.append(focusInput, focusResults);
  const setFocusLabel = () => {
    const f = state.focus ? store.byId.get(state.focus) : null;
    focusInput.value = f && f.kind === "element" ? elem(f.i).name : "";
  };
  setFocusLabel();
  focusInput.addEventListener("input", () => {
    const hits = search(focusInput.value, 12).elements;
    clear(focusResults);
    if (!hits.length || !focusInput.value.trim()) { focusResults.hidden = true; return; }
    for (const { i } of hits) {
      const e = elem(i);
      focusResults.appendChild(h("a", { href: "#", onclick: (ev) => { ev.preventDefault(); state.focus = e.id; state.all = false; state.expanded.clear(); state.confirmed = false; push(); setFocusLabel(); focusResults.hidden = true; build(); } },
        typeIcon(e.type), h("span", { class: "ellipsis" }, e.name), h("span", { class: "type" }, e.type)));
    }
    focusResults.hidden = false;
  });
  focusInput.addEventListener("keydown", (ev) => { if (ev.key === "Escape") { focusResults.hidden = true; } if (ev.key === "Enter") { focusResults.querySelector("a")?.click(); } });
  focusInput.addEventListener("blur", () => setTimeout(() => (focusResults.hidden = true), 150));

  const depthSel = h("select", { class: "input", onchange: (e) => { state.depth = +e.target.value; push(); build(); } },
    [1, 2, 3, 4].map((n) => h("option", { value: n, selected: state.depth === n }, `depth ${n}`)));
  const dirGroup = h("div", { class: "btn-group" }, ["out", "both", "in"].map((d) =>
    h("button", { class: "btn sm" + (state.dir === d ? " active" : ""), dataset: { dir: d }, onclick: () => { state.dir = d; push(); dirGroup.querySelectorAll(".btn").forEach((b) => b.classList.toggle("active", b.dataset.dir === d)); build(); } }, d === "both" ? "↔" : d === "out" ? "→ out" : "← in")));
  const allBtn = h("button", { class: "btn sm" + (state.all ? " active" : ""), onclick: () => { state.all = !state.all; state.confirmed = false; allBtn.classList.toggle("active", state.all); push(); build(); } }, "Whole model");
  const relayoutBtn = h("button", { class: "btn sm", onclick: () => { for (const n of nodes) { n.fx = null; n.fy = null; } sim?.reheat(1); } }, "Re-layout");
  const countLabel = h("span", { class: "muted small nowrap" });
  head.append(focusBox, depthSel, dirGroup, allBtn, h("span", { class: "spacer" }), countLabel, relayoutBtn);

  const layerChips = h("div", { class: "chips" });
  const relChips = h("div", { class: "chips" });
  filters.append(h("span", { class: "muted small" }, "Layers"), layerChips, h("span", { class: "muted small", style: { marginLeft: "12px" } }, "Relations"), relChips);
  const chip = (label, key, set, fill) => h("button", {
    class: "chip" + (set.has(key) ? "" : " active"),
    onclick: (ev) => { if (set.has(key)) set.delete(key); else set.add(key); ev.currentTarget.classList.toggle("active", !set.has(key)); push(); build(); },
  }, fill ? h("span", { class: "swatch", style: { background: fill, marginRight: "5px", verticalAlign: "-1px" } }) : null, label);
  const presentLayers = LAYERS.filter((l) => new Set(store.data.elements.map((e) => e.layer)).has(l));
  const presentRels = [...new Set(store.data.relations.map((r) => r.type))].sort();
  // "All" and "None" on each row: with a dozen types, toggling one at a time
  // is the wrong tool for "show me only Serving".
  const allNone = (keys, set, container) => [
    h("button", { class: "chip", title: "Show all", onclick: () => { set.clear(); syncChips(container, set); push(); build(); } }, "All"),
    h("button", { class: "chip", title: "Hide all", onclick: () => { for (const k of keys) set.add(k); syncChips(container, set); push(); build(); } }, "None"),
    h("span", { class: "muted", style: { margin: "0 4px" } }, "·"),
  ];
  const syncChips = (container, set) => container.querySelectorAll(".chip[data-key]").forEach((c) => c.classList.toggle("active", !set.has(c.dataset.key)));
  layerChips.append(...allNone(presentLayers, state.hideLayers, layerChips));
  for (const l of presentLayers) { const c = chip(LAYER_LABEL[l] || l, l, state.hideLayers, layerFill(l)); c.dataset.key = l; layerChips.appendChild(c); }
  relChips.append(...allNone(presentRels, state.hideRels, relChips));
  for (const t of presentRels) { const c = chip(relLabel(t), t, state.hideRels); c.dataset.key = t; relChips.appendChild(c); }

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
  const pz = attachPanZoom(svg, canvas, { isNodeTarget: (t) => !!t.closest?.(".node") });
  pz.fit({ x: -400, y: -300, w: 800, h: 600 });
  hud.append(
    h("button", { class: "btn sm", onclick: () => fitAll(), title: "Fit" }, "Fit"),
    h("button", { class: "btn sm", onclick: () => pz.zoomIn() }, "+"),
    h("button", { class: "btn sm", onclick: () => pz.zoomOut() }, "−"),
  );

  let nodes = [], links = [], sim = null;
  const nodeByIdx = new Map();

  function build() {
    sim?.stop();
    const set = chooseNodes();
    if (set === null) return; // message shown
    const old = nodeByIdx;
    nodeByIdx.clear();
    nodes = [];
    for (const i of set) {
      const e = elem(i);
      const prev = old.get(i);
      // Wide enough for the longest word and for the name on two lines,
      // taller when it still needs a third.
      const name = e.name || "?";
      const longest = Math.max(...name.split(/\s+/).map((wd) => wd.length));
      const w = clamp(Math.max(longest * PER_CHAR + 48, Math.min((name.length * PER_CHAR) / 2 + 48, 190)), 100, 200);
      const h = wrap(name, w - 44, 3).length >= 3 ? NODE_H + 13 : NODE_H;
      const n = prev || { i, id: e.id, type: e.type, name: e.name, w, h };
      n.w = w; n.h = h;
      nodes.push(n);
      nodeByIdx.set(i, n);
    }
    // New nodes start beside a neighbour that already had a place, so an
    // expansion grows out of the node that was opened rather than exploding.
    for (const n of nodes) {
      if (n.x !== undefined) continue;
      const near = [...store.out[n.i], ...store.inc[n.i]].map((ri) => nodeByIdx.get(otherEnd(rel(ri), n.i))).find((m) => m && m.x !== undefined);
      if (near) { n.x = near.x + (Math.random() - 0.5) * 60; n.y = near.y + (Math.random() - 0.5) * 60; }
    }
    links = [];
    const pairCount = new Map();
    store.data.relations.forEach((r, ri) => {
      if (r.src < 0 || r.tgt < 0 || r.src === r.tgt) return;
      if (state.hideRels.has(r.type)) return;
      const a = nodeByIdx.get(r.src), b = nodeByIdx.get(r.tgt);
      if (!a || !b) return;
      const key = r.src < r.tgt ? `${r.src}-${r.tgt}` : `${r.tgt}-${r.src}`;
      const k = pairCount.get(key) || 0;
      pairCount.set(key, k + 1);
      links.push({ ri, r, source: a, target: b, k, flip: r.src > r.tgt });
    });
    // Spread parallel edges symmetrically about the centre line.
    for (const l of links) { const total = pairCount.get(l.source.i < l.target.i ? `${l.source.i}-${l.target.i}` : `${l.target.i}-${l.source.i}`); l.offset = (l.k - (total - 1) / 2) * 14 * (l.flip ? -1 : 1); }

    countLabel.textContent = `${fmt(nodes.length)} nodes · ${fmt(links.length)} edges`;
    draw();
    const fresh = nodes.some((n) => n.vx === undefined);
    sim = createSimulation(nodes, links, { onTick: position });
    if (fresh) sim.settle(nodes.length > 200 ? 120 : 250);
    position();
    fitAll();
    sim.reheat(0.3);
    renderLegend();
  }

  // Which elements to show. `null` means a message was shown instead.
  function chooseNodes() {
    msg.hidden = true;
    clear(gEdges); clear(gNodes); nodes = []; links = [];
    const shown = (i) => !state.hideLayers.has(elem(i).layer);
    const passRel = (r) => !state.hideRels.has(r.type);
    let set;
    if (state.all) {
      set = new Set();
      store.data.elements.forEach((_, i) => { if (shown(i)) set.add(i); });
    } else if (state.focus) {
      const f = store.byId.get(state.focus);
      if (!f || f.kind !== "element") { showMessage(`No element has id ${state.focus}.`); return null; }
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
    // Expansions: every neighbour of an opened node, whatever the depth.
    for (const i of state.expanded) {
      if (!set.has(i)) continue;
      for (const ri of [...store.out[i], ...store.inc[i]]) {
        const o = otherEnd(rel(ri), i);
        if (o >= 0 && passRel(rel(ri)) && shown(o)) set.add(o);
      }
    }
    if (set.size === 0) {
      const top = store.data.elements.map((_, i) => [i, store.out[i].length + store.inc[i].length]).sort((a, b) => b[1] - a[1]).slice(0, 8);
      showMessage(h("div", null,
        h("p", null, "Start from an element, or draw the whole model."),
        top.length ? h("div", { class: "link-list", style: { marginTop: "12px", textAlign: "left" } },
          h("div", { class: "muted small", style: { padding: "0 6px 4px" } }, "Most connected"),
          top.map(([i, deg]) => h("a", { href: "#", onclick: (ev) => { ev.preventDefault(); state.focus = elem(i).id; state.confirmed = false; push(); setFocusLabel(); build(); } }, typeIcon(elem(i).type), h("span", { class: "ellipsis" }, elem(i).name), h("span", { class: "muted small", style: { marginLeft: "auto" } }, `${deg}`)))) : null,
      ));
      return null;
    }
    if (set.size > HARD_CAP) {
      showMessage(`That is ${fmt(set.size)} elements — more than the ${fmt(HARD_CAP)} this graph will draw. Hide some layers, lower the depth, or start from an element.`);
      return null;
    }
    if (set.size > WARN_AT && !state.confirmed) {
      showMessage(h("div", null,
        h("p", null, `${fmt(set.size)} elements is a lot to read at once.`),
        h("div", { class: "actions", style: { justifyContent: "center", marginTop: "10px" } },
          h("button", { class: "btn sm primary", onclick: () => { state.confirmed = true; build(); } }, "Draw it anyway"),
          h("button", { class: "btn sm", onclick: () => { state.all = false; allBtn.classList.remove("active"); push(); build(); } }, "Never mind")),
      ));
      return null;
    }
    return set;
  }

  function showMessage(content) {
    clear(msg);
    msg.append(typeof content === "string" ? document.createTextNode(content) : content);
    msg.hidden = false;
    countLabel.textContent = "";
    clear(legend);
  }

  // ---- drawing ----------------------------------------------------------
  function draw() {
    clear(gEdges); clear(gNodes);
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
      if (n.i === store.byId.get(state.focus)?.i) g.classList.add("focus");
      if (n.fx !== undefined && n.fx !== null) g.classList.add("pinned");
      if (state.selected === n.id) g.classList.add("selected");
      g.appendChild(s("title", null, `${n.name}\n${n.type}${state.expanded.has(n.i) ? "" : " — double-click to expand"}`));
      wireNode(g, n);
      n.el = g;
      gNodes.appendChild(g);
    }
  }

  function position() {
    for (const n of nodes) n.el?.setAttribute("transform", `translate(${(n.x - n.w / 2).toFixed(1)},${(n.y - n.h / 2).toFixed(1)})`);
    for (const l of links) {
      const a = l.source, b = l.target;
      let dx = b.x - a.x, dy = b.y - a.y;
      const d = Math.sqrt(dx * dx + dy * dy) || 1;
      // Parallel edges are pushed sideways; then each end is clipped to the
      // border of its box so the arrowhead lands on the outline.
      const ox = (-dy / d) * (l.offset || 0), oy = (dx / d) * (l.offset || 0);
      const p1 = clipToRect(a.x + ox, a.y + oy, b.x + ox, b.y + oy, a);
      const p2 = clipToRect(b.x + ox, b.y + oy, a.x + ox, a.y + oy, b);
      for (const ln of [l.line, l.hit]) { ln.setAttribute("x1", p1.x.toFixed(1)); ln.setAttribute("y1", p1.y.toFixed(1)); ln.setAttribute("x2", p2.x.toFixed(1)); ln.setAttribute("y2", p2.y.toFixed(1)); }
      if (l.label) { l.label.setAttribute("x", ((p1.x + p2.x) / 2).toFixed(1)); l.label.setAttribute("y", ((p1.y + p2.y) / 2 - 4).toFixed(1)); }
    }
  }

  // Where the ray from (x,y) toward (tx,ty) leaves the box centred on n.
  function clipToRect(x, y, tx, ty, n) {
    const dx = tx - x, dy = ty - y;
    if (dx === 0 && dy === 0) return { x, y };
    const hw = n.w / 2, hh = n.h / 2;
    const cx = n.x, cy = n.y;
    // Ray from the box centre parallel to the edge, shifted by the offset.
    const sx = x - cx, sy = y - cy;
    let t = Infinity;
    if (dx !== 0) { const tt = ((dx > 0 ? hw : -hw) - sx) / dx; if (tt > 0) t = Math.min(t, tt); }
    if (dy !== 0) { const tt = ((dy > 0 ? hh : -hh) - sy) / dy; if (tt > 0) t = Math.min(t, tt); }
    if (!isFinite(t)) t = 0;
    return { x: x + dx * t, y: y + dy * t };
  }

  function wireNode(g, n) {
    let drag = null;
    g.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      ev.stopPropagation();
      const p = pz.toSvg(ev.clientX, ev.clientY);
      drag = { dx: n.x - p.x, dy: n.y - p.y, moved: false, sx: ev.clientX, sy: ev.clientY };
      g.setPointerCapture(ev.pointerId);
    });
    g.addEventListener("pointermove", (ev) => {
      if (!drag) return;
      if (Math.abs(ev.clientX - drag.sx) + Math.abs(ev.clientY - drag.sy) > 3) drag.moved = true;
      if (!drag.moved) return;
      const p = pz.toSvg(ev.clientX, ev.clientY);
      n.fx = p.x + drag.dx; n.fy = p.y + drag.dy; n.x = n.fx; n.y = n.fy;
      g.classList.add("pinned");
      if (!sim.running) position(); else sim.reheat(0.15);
    });
    const up = (ev) => {
      if (!drag) return;
      const moved = drag.moved;
      drag = null;
      g.releasePointerCapture?.(ev.pointerId);
      if (moved) { sim.reheat(0.1); return; }
    };
    g.addEventListener("pointerup", up);
    g.addEventListener("pointercancel", up);
    g.addEventListener("click", (ev) => {
      ev.stopPropagation();
      setSelected(n.id, g);
      select(n.id);
    });
    g.addEventListener("dblclick", (ev) => {
      ev.stopPropagation();
      if (state.expanded.has(n.i)) state.expanded.delete(n.i); else state.expanded.add(n.i);
      build();
    });
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

  function fitAll() {
    if (!nodes.length) return;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const n of nodes) { x0 = Math.min(x0, n.x - n.w / 2); y0 = Math.min(y0, n.y - n.h / 2); x1 = Math.max(x1, n.x + n.w / 2); y1 = Math.max(y1, n.y + n.h / 2); }
    pz.fit({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 }, 40);
  }

  function renderLegend() {
    clear(legend);
    const present = new Map();
    for (const n of nodes) present.set(typeOf(n.type).layer, layerFill(typeOf(n.type).layer));
    for (const l of LAYERS) if (present.has(l)) legend.append(h("span", null, h("span", { class: "swatch", style: { background: present.get(l) } }), LAYER_LABEL[l] || l));
    legend.hidden = present.size === 0;
  }

  build();
  return () => { sim?.stop(); pz.destroy(); };
}

function layerFill(layer) {
  for (const t of Object.values(store.data.types)) if (t.layer === layer) return t.fill;
  return "#ffffff";
}

function clamp(v, lo, hi) { return Math.max(lo, Math.min(hi, v)); }
