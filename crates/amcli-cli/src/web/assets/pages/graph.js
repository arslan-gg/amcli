// The graph: a view of the model that was never saved.
//
// The page decides *what* to draw — it holds the whole model already, so a
// filter costs nothing — and asks the server *where*, because the answer is
// `amcli-view`'s layout, the code `view auto` runs. So the picture here is the
// picture `amcli view auto --from <centre> -n <depth>` would file in the
// model: same rows, same box sizes, same straight lines.
//
// The drawing is not draggable, for the same reason a saved view is not: where
// a box goes is the layout's answer, and moving one by hand only makes the
// picture disagree with what amcli would draw.

import { h, s, clear, fmt, relLabel } from "../dom.js";
import { store, elem, rel, search } from "../store.js";
import { ensureSymbols, nodeGroup, relStyle, typeOf, typeIcon } from "../notation.js";
import { icon } from "../icons.js";
import { toolbar, filterBar, chip, button, iconButton, segmented, selectField, searchField, emptyState, hint, anchorTo } from "../ui.js";
import { replaceParams } from "../router.js";
import { mark } from "../fuzzy.js";
import { attachPanZoom } from "../panzoom.js";
import { select, selectedId, clearSelection, railContext } from "../app.js";
import { pins, isPinned, setPin, clearPins, replacePins, onPins } from "../pins.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];
const WARN_AT = 400;   // ask before drawing more than this
const HARD_CAP = 600;  // what the server will lay out, and what a person can read

export function mount(main, route) {
  const p = route.params;
  const state = {
    focus: p.get("focus") || "",
    depth: clamp(parseInt(p.get("depth") || "2", 10) || 2, 1, 4),
    dir: ["in", "out", "both"].includes(p.get("dir")) ? p.get("dir") : "both",
    confirmed: false,
  };
  // Only a link that actually carries pins may replace them. Arriving at the
  // graph with none in the URL means "no opinion", not "unpin everything" —
  // the reader may have pinned from the inspector on another page.
  if (p.has("pin")) replacePins((p.get("pin") || "").split(",").filter(Boolean));

  const hidden = {
    layer: new Set((p.get("no_layer") || "").split(",").filter(Boolean)),
    type: new Set((p.get("no_type") || "").split(",").filter(Boolean)),
    kind: new Set((p.get("no_kind") || "").split(",").filter(Boolean)),
  };
  const push = () => replaceParams({
    focus: state.focus,
    depth: state.depth === 2 ? "" : String(state.depth),
    dir: state.dir === "both" ? "" : state.dir,
    pin: pins().join(","),
    no_layer: [...hidden.layer].join(","),
    no_type: [...hidden.type].join(","),
    no_kind: [...hidden.kind].join(","),
  });

  /* ---- the toolbar ------------------------------------------------------- */
  // The centre is a chip, not text left in the search box: typing a name is
  // how you look for a new centre, not a record of the one you have.
  const centre = h("span", { class: "graph-centre" });
  const finder = searchField({
    value: "", placeholder: "Centre on…", width: "var(--ctl-w-sm)",
    oninput: (v) => showHits(v),
  });
  const hits = h("div", { class: "popover", hidden: true });
  const finderBox = h("div", { class: "popover-anchor" }, finder, hits);

  const depth = selectField({
    value: String(state.depth), title: "How many hops out from the centre",
    options: [1, 2, 3, 4].map((n) => ({ value: String(n), label: `${n} hop${n > 1 ? "s" : ""}` })),
    onchange: (v) => { state.depth = +v; state.confirmed = false; push(); build(); },
  });
  const dir = segmented([
    { value: "out", iconName: "arrow-right", label: "", title: "Follow relationships outwards only" },
    { value: "both", iconName: "arrow-both", label: "", title: "Follow relationships either way" },
    { value: "in", iconName: "arrow-left", label: "", title: "Follow relationships inwards only" },
  ], state.dir, (v) => { state.dir = v; state.confirmed = false; push(); build(); });
  const meta = h("span", { class: "nowrap" });

  const bar = toolbar({
    title: "Graph", titleIcon: "graph", meta,
    controls: [centre, finderBox, depth, dir],
    trailing: [
      button({ iconName: "download", label: "SVG", title: "Download the graph as it is drawn", onclick: () => saveSvg() }),
      button({ iconName: "download", label: "PNG", title: "Download the graph as it is drawn, at 2× resolution", onclick: () => savePng() }),
    ],
  });

  /* ---- the filters ------------------------------------------------------- */
  const counts = (list) => { const m = new Map(); for (const v of list) m.set(v, (m.get(v) || 0) + 1); return m; };
  const layerN = counts(store.data.elements.map((e) => e.layer));
  const typeN = counts(store.data.elements.map((e) => e.type));
  const kindN = counts(store.data.relations.map((r) => r.type));

  const dims = [
    {
      key: "layer", label: "Layers", noun: "layers", hidden: hidden.layer,
      values: () => LAYERS.filter((l) => layerN.has(l)).map((l) => ({ value: l, label: l, count: layerN.get(l), swatch: layerFill(l) })),
      onChange: () => { state.confirmed = false; push(); filters.redraw(); build({ keep: true }); },
    },
    {
      key: "type", label: "Types", noun: "types", hidden: hidden.type,
      values: () => [...typeN.keys()].filter((t) => !hidden.layer.has(typeOf(t).layer)).sort()
        .map((t) => ({ value: t, label: t, count: typeN.get(t), swatch: typeOf(t).fill })),
      onChange: () => { state.confirmed = false; push(); build({ keep: true }); },
    },
    {
      key: "kind", label: "Kinds", noun: "kinds", hidden: hidden.kind,
      values: () => [...kindN.keys()].sort().map((t) => ({ value: t, label: relLabel(t), count: kindN.get(t) })),
      onChange: () => { state.confirmed = false; push(); build({ keep: true }); },
    },
  ];
  const filters = filterBar(dims);

  // Pinned elements are an override, not a dimension: whatever the menus say,
  // a pinned box is on the graph. They get the same chip, in the same bar, so
  // they read as part of the same control — but no menu, because the set is
  // built by shift-clicking a box.
  const pinRow = h("div", { class: "filter-row" });
  filters.append(h("span", { class: "filter-key" }, "Pinned"), pinRow);

  function drawPins() {
    clear(pinRow);
    const live = pins().filter((id) => store.byId.get(id)?.kind === "element");
    if (!live.length) {
      pinRow.appendChild(hint("Nothing pinned. Shift-click a box, or use Pin in the details panel — a pinned element stays on the graph even when a filter or the hop limit would drop it."));
      return;
    }
    for (const id of live) {
      const e = elem(store.byId.get(id).i);
      pinRow.appendChild(chip({
        label: e.name, removable: true,
        title: `Show ${e.name}`,
        onclick: () => select(id),
        onRemove: () => setPin(id, false),
      }));
    }
    if (live.length > 1) pinRow.appendChild(chip({ label: "Unpin all", onclick: clearPins }));
  }
  const stopWatchingPins = onPins(() => { push(); drawPins(); build({ keep: true }); });

  /* ---- the canvas -------------------------------------------------------- */
  const canvas = h("div", { class: "canvas" });
  const svg = s("svg", { class: "graph" });
  ensureSymbols(svg);
  const gEdges = s("g", { class: "edges" });
  const gNodes = s("g", { class: "nodes" });
  svg.append(gEdges, gNodes);
  const hud = h("div", { class: "canvas-hud" });
  const legend = h("div", { class: "legend", hidden: true });
  const msg = h("div", { class: "canvas-msg", hidden: true });
  canvas.append(svg, hud, legend, msg);

  const page = h("div", { class: "page" }, bar, canvas);
  main.appendChild(page);

  // The filters live in the rail, the same place and the same shape as on
  // every other page — and the canvas gets the whole of the middle.
  railContext().appendChild(h("div", { class: "rail-group" },
    h("h2", { class: "caps rail-group-title" }, "Filter"), filters));

  // The floor stops a drawing far wider than the pane being shrunk until every
  // box is a fraction of a pixel and the canvas looks empty.
  const pz = attachPanZoom(svg, canvas, { maxFitScale: 1.25, minFitScale: 0.14 });
  hud.append(
    iconButton("fit", "Fit to the window", () => fitAll()),
    iconButton("plus", "Zoom in", () => pz.zoomIn()),
    iconButton("minus", "Zoom out", () => pz.zoomOut()));

  let nodes = [], links = [];
  let generation = 0;
  let alive = true;

  /* ---- centring ---------------------------------------------------------- */
  // The whole model is not centred on anything: the node set is every element
  // the filters allow, and it is read without the focus, the depth or the
  // direction. So while that is on, the centre says nothing and the two
  // controls that walk out from a centre are disabled — a control you can
  // work that changes nothing is worse than one you cannot.
  // With nothing centred there is nothing to count hops from, so the two
  // controls that only mean something relative to a centre say so.
  function scopeControls() {
    const none = !state.focus;
    depth.disabled = none;
    dir.querySelectorAll(".btn").forEach((b) => { b.disabled = none; });
  }

  function drawCentre() {
    clear(centre);
    const f = state.focus ? store.byId.get(state.focus) : null;
    if (f && f.kind === "element") {
      const e = elem(f.i);
      const box = h("span", { class: "badge is-solid", title: `Centred on ${e.name}` },
        typeIcon(e.type), h("span", { class: "ellipsis" }, e.name),
        h("button", {
          class: "badge-x", type: "button", title: "Stop centring here", "aria-label": "Stop centring here",
          onclick: () => { state.focus = ""; state.confirmed = false; push(); drawCentre(); build(); },
        }, icon("close")));
      centre.appendChild(box);
    } else {
      // Not an absence but a mode: with no centre the graph is the whole model.
      centre.appendChild(h("span", { class: "badge", title: "Every element the filters allow. Centre on one to see its neighbourhood." }, "Whole model"));
    }
  }

  function recentre(id) {
    state.focus = id;
    state.confirmed = false;
    finder.input.value = "";
    shutHits();
    push(); scopeControls(); drawCentre(); build();
  }

  // The hits list is the one floating panel the page builds itself, because
  // popover() opens on a click and this one opens on a keystroke. It still owes
  // the reader everything popover() gives: it shuts when you point somewhere
  // else, it follows its field when the window moves or scrolls, and every row
  // is reachable from the keyboard — not just the first.
  //
  // Closing on the field's own blur is what it must not do: panzoom calls
  // preventDefault on every pointerdown in the canvas, so clicking the drawing
  // never moves focus, the blur never fires, and the panel is left parked over
  // the picture.
  let hitAt = -1;
  const hitRows = () => [...hits.querySelectorAll("a")];
  const shutHits = () => { hits.hidden = true; hitAt = -1; };
  const outsideHits = (ev) => { if (!finderBox.contains(ev.target)) shutHits(); };
  const moveHits = () => { if (!hits.hidden) anchorTo(hits, finder); };
  document.addEventListener("pointerdown", outsideHits);
  window.addEventListener("resize", moveHits);
  window.addEventListener("scroll", moveHits, true);

  function showHits(q) {
    clear(hits);
    hitAt = -1;
    const found = q.trim() ? search(q, 10).elements : [];
    if (!found.length) { shutHits(); return; }
    const list = h("div", { class: "popover-list" });
    for (const { i } of found) {
      const e = elem(i);
      list.appendChild(h("a", {
        class: "palette-hit", href: "#",
        onclick: (ev) => { ev.preventDefault(); recentre(e.id); },
      }, typeIcon(e.type), h("span", { class: "ellipsis" }, mark(e.name || "", q)), h("span", { class: "hit-type" }, e.type)));
    }
    hits.appendChild(list);
    hits.hidden = false;
    anchorTo(hits, finder);
  }

  function markHit(at) {
    const rows = hitRows();
    if (!rows.length) return;
    hitAt = (at + rows.length) % rows.length;
    rows.forEach((r, k) => r.classList.toggle("is-active", k === hitAt));
    rows[hitAt].scrollIntoView({ block: "nearest" });
  }

  finder.input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { shutHits(); return; }
    if (hits.hidden) return;
    if (e.key === "ArrowDown") { e.preventDefault(); markHit(hitAt + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); markHit(hitAt - 1); }
    else if (e.key === "Enter") { e.preventDefault(); (hitRows()[hitAt] || hitRows()[0])?.click(); }
  });

  /* ---- building ---------------------------------------------------------- */
  // `keep` asks the redraw not to move the camera. A redraw the reader did not
  // ask to move must not move: pinning one box adds one node to the picture,
  // and refitting the whole graph for it threw away the scale and the corner
  // they had navigated to. Narrowing — a pin, a filter — keeps the camera;
  // asking for a different neighbourhood — another centre, more hops, the
  // other way round — fits, because that is a different picture. The one
  // exception is in `draw`.
  function build({ keep = false } = {}) {
    const set = chooseNodes();
    if (set === null) return;
    const want = [...set].sort((a, b) => a - b);
    const mine = ++generation;
    showMessage(emptyState({ title: "Laying out…", body: "Asking amcli where the boxes go." }));
    const url = `/api/layout?e=${encodeURIComponent(compact(want))}`
      + `&hiderel=${encodeURIComponent([...hidden.kind].join(","))}`
      + `&c=${encodeURIComponent(store.checksum)}`;
    fetch(url, { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : r.json().then((e) => Promise.reject(new Error(e.error || `HTTP ${r.status}`)))))
      .then((placed) => {
        if (!alive || mine !== generation) return;
        msg.hidden = true;
        draw(want, placed, keep);
      })
      .catch((e) => {
        if (!alive || mine !== generation) return;
        showMessage(emptyState({ iconName: "alert", title: "Could not lay this out", body: e.message }));
      });
  }

  function chooseNodes() {
    msg.hidden = true;
    const shown = (i) => !hidden.layer.has(elem(i).layer) && !hidden.type.has(elem(i).type);
    const passRel = (r) => !hidden.kind.has(r.type);
    let set;
    if (state.focus) {
      const f = store.byId.get(state.focus);
      if (!f || f.kind !== "element") { emptyGraph(`Nothing in this model has id ${state.focus}.`); return null; }
      if (!shown(f.i)) { emptyGraph("The centre is in a layer or of a type this graph is hiding."); return null; }
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
      store.data.elements.forEach((_, i) => { if (shown(i)) set.add(i); });
    }
    for (const id of pins()) {
      const f = store.byId.get(id);
      if (f && f.kind === "element") set.add(f.i);
    }

    if (set.size === 0) { emptyGraph("Every element is hidden by a filter. Show a layer or a type."); return null; }
    if (set.size > HARD_CAP) {
      showMessage(emptyState({
        iconName: "alert",
        title: `${fmt(set.size)} elements is more than this graph will draw`,
        body: `The limit is ${fmt(HARD_CAP)}. Hide a layer or a type, lower the depth, or centre on an element.`,
      }));
      return null;
    }
    if (set.size > WARN_AT && !state.confirmed) {
      showMessage(emptyState({
        title: `${fmt(set.size)} elements is a lot to read at once`,
        body: "Every box will be there, but most will be too small to read without zooming.",
        actions: [
          button({ label: "Draw it anyway", variant: "primary", onclick: () => { state.confirmed = true; build(); } }),
          button({ label: "Never mind", onclick: () => { finder.input.focus(); } }),
        ],
      }));
      return null;
    }
    return set;
  }

  function emptyGraph(text) {
    clear(gEdges); clear(gNodes);
    nodes = []; links = [];
    legend.hidden = true;
    showMessage(emptyState({ iconName: "alert", title: "Nothing to draw", body: text }));
  }

  function showMessage(content) {
    clear(msg).append(content);
    msg.hidden = false;
  }

  /* ---- drawing ----------------------------------------------------------- */
  function draw(want, placed, keep) {
    clear(gEdges); clear(gNodes);
    nodes = want.map((i, at) => {
      const [x, y, w, hh] = placed.nodes[at];
      const e = elem(i);
      return { i, id: e.id, type: e.type, name: e.name, x, y, w, h: hh };
    });
    links = [];
    for (const [ri, a, b] of placed.edges) {
      if (a === b) continue;
      links.push({ ri, r: rel(ri), source: nodes[a], target: nodes[b] });
    }
    const box = extent();
    meta.textContent = `${fmt(nodes.length)} · ${fmt(links.length)}`;
    meta.title = `${fmt(nodes.length)} elements, ${fmt(links.length)} relationships, laid out by amcli-view — the same code \`view auto\` runs.`;

    for (const l of links) {
      const st = relStyle(l.r);
      const g = s("g", { class: "edge", dataset: { rel: l.r.id } });
      const line = s("line", {
        "stroke-dasharray": st.dash || null,
        "marker-end": st.target !== "none" ? `url(#m-${st.target}-end)` : null,
        "marker-start": st.source !== "none" ? `url(#m-${st.source}-start)` : null,
      });
      const hit = s("line", { class: "hit" });
      g.append(line, hit);
      if (l.r.name) g.appendChild(s("text", { "text-anchor": "middle" }, l.r.name));
      g.addEventListener("click", (ev) => {
        ev.stopPropagation();
        if (canvas.dataset.justDragged) return;
        markSelection(l.r.id);
        select(l.r.id);
      });
      l.el = g; l.line = line; l.hit = hit; l.label = g.querySelector("text");
      gEdges.appendChild(g);
    }
    for (const n of nodes) {
      const g = nodeGroup(n.type, n.name, n.w, n.h);
      g.dataset.id = n.id;
      g.setAttribute("transform", `translate(${n.x},${n.y})`);
      if (n.id === state.focus) g.classList.add("is-focus");
      if (isPinned(n.id)) g.classList.add("is-pinned");
      const how = isPinned(n.id)
        ? "shift-click to unpin"
        : "double-click to centre here · shift-click to pin";
      g.appendChild(s("title", null, `${n.name}\n${n.type} — ${how}`));
      g.addEventListener("click", (ev) => {
        ev.stopPropagation();
        if (canvas.dataset.justDragged) return;
        if (ev.shiftKey) { setPin(n.id, !isPinned(n.id)); return; }
        markSelection(n.id);
        select(n.id);
      });
      g.addEventListener("dblclick", (ev) => { ev.stopPropagation(); ev.preventDefault(); recentre(n.id); });
      n.el = g;
      gNodes.appendChild(g);
    }
    position();
    // The exception to `keep`: a camera left pointing at nothing. The server
    // lays the whole picture out again for every redraw, so a graph that has
    // just lost most of its nodes to a filter can end up nowhere near where
    // the reader was looking — and a blank sheet is worse than a moved one.
    if (!keep || !meets(pz.viewBox, box)) fitAll();
    drawLegend();
    markSelection(selectedId());
  }

  function position() {
    for (const l of links) {
      const a = centreOf(l.source), b = centreOf(l.target);
      const p1 = clipToRect(a, b, l.source);
      const p2 = clipToRect(b, a, l.target);
      for (const ln of [l.line, l.hit]) {
        ln.setAttribute("x1", p1.x.toFixed(1)); ln.setAttribute("y1", p1.y.toFixed(1));
        ln.setAttribute("x2", p2.x.toFixed(1)); ln.setAttribute("y2", p2.y.toFixed(1));
      }
      if (l.label) {
        l.label.setAttribute("x", ((p1.x + p2.x) / 2).toFixed(1));
        l.label.setAttribute("y", ((p1.y + p2.y) / 2 - 4).toFixed(1));
      }
    }
  }

  const centreOf = (n) => ({ x: n.x + n.w / 2, y: n.y + n.h / 2 });

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

  // The neighbourhood of the selected node stands out and the rest recedes —
  // recedes, not vanishes: at the old opacity the graph read as broken.
  function markSelection(id) {
    svg.querySelectorAll(".is-selected, .is-dim").forEach((x) => x.classList.remove("is-selected", "is-dim"));
    if (!id) return;
    const n = nodes.find((x) => x.id === id);
    const l = links.find((x) => x.r.id === id);
    (n?.el || l?.el)?.classList.add("is-selected");
    if (!n) return;
    const keep = new Set([n]);
    for (const link of links) {
      if (link.source === n) keep.add(link.target);
      if (link.target === n) keep.add(link.source);
    }
    for (const m of nodes) if (!keep.has(m)) m.el.classList.add("is-dim");
    for (const link of links) if (link.source !== n && link.target !== n) link.el.classList.add("is-dim");
  }

  // Blank canvas clears the selection through the shell, not just the drawing:
  // dimming the picture while the inspector goes on describing the old concept
  // leaves it selected, and the next filter change draws it highlighted again.
  svg.addEventListener("click", () => {
    if (canvas.dataset.justDragged) return;
    clearSelection();
  });
  const onSelect = (e) => markSelection(e.detail.id);
  document.addEventListener("amcli:select", onSelect);

  function extent() {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const n of nodes) { x0 = Math.min(x0, n.x); y0 = Math.min(y0, n.y); x1 = Math.max(x1, n.x + n.w); y1 = Math.max(y1, n.y + n.h); }
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  }

  function fitAll() {
    if (!nodes.length) return;
    pz.fit(extent(), 40);
  }

  // Do the camera and the drawing have any pixel in common?
  const meets = (a, b) =>
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

  /* ---- taking it away ----------------------------------------------------
     A graph is not saved, so there is no id for the server to render from and
     no URL to open: what leaves is the drawing on the screen. The clone carries
     its own stylesheet with the tokens resolved, because a file opened outside
     this page has no :root to read them from, and it drops the parts that only
     exist to be clicked. */
  const EXPORT_PAD = 24;

  function standalone() {
    const box = extent();
    const clone = svg.cloneNode(true);
    clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    clone.setAttribute("viewBox", `${box.x - EXPORT_PAD} ${box.y - EXPORT_PAD} ${box.w + EXPORT_PAD * 2} ${box.h + EXPORT_PAD * 2}`);
    clone.setAttribute("width", box.w + EXPORT_PAD * 2);
    clone.setAttribute("height", box.h + EXPORT_PAD * 2);
    clone.removeAttribute("class");
    clone.querySelectorAll(".hit, title").forEach((n) => n.remove());
    clone.querySelectorAll(".is-dim, .is-selected").forEach((n) => n.classList.remove("is-dim", "is-selected"));

    const css = getComputedStyle(document.documentElement);
    const tok = (name) => css.getPropertyValue(name).trim();
    const style = s("style", null,
      `svg{background:${tok("--paper")}}` +
      `.node text{font-family:${tok("--font")};font-size:${tok("--fs-xs")};fill:${tok("--paper-ink")}}` +
      `.node .figure{stroke-width:1}` +
      `.node.is-focus .figure{stroke:${tok("--paper-ink")};stroke-width:2.5}` +
      `.node.is-pinned .figure{stroke:${tok("--paper-ink")};stroke-width:2;stroke-dasharray:4 2}` +
      `.edge line{stroke:${tok("--edge")};stroke-width:1}` +
      `.edge text{font-family:${tok("--font")};font-size:${tok("--fs-xs")};fill:${tok("--edge")}}` +
      `marker path,marker polygon,marker polyline,marker circle{stroke:${tok("--edge")}}` +
      `marker .filled{fill:${tok("--edge")}}` +
      `marker .hollow{fill:${tok("--paper")}}`);
    clone.insertBefore(style, clone.firstChild);
    return { text: new XMLSerializer().serializeToString(clone), w: box.w + EXPORT_PAD * 2, h: box.h + EXPORT_PAD * 2 };
  }

  function fileName() {
    const centred = state.focus && store.byId.get(state.focus)?.kind === "element"
      ? elem(store.byId.get(state.focus).i).name
      : "whole model";
    return `${store.data.model.name || "model"} — ${centred} (graph)`.replace(/[\\/:*?"<>|]+/g, "_").trim();
  }

  function offer(blob, name) {
    const url = URL.createObjectURL(blob);
    const a = h("a", { href: url, download: name });
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function saveSvg() {
    if (!nodes.length) return;
    offer(new Blob([standalone().text], { type: "image/svg+xml" }), `${fileName()}.svg`);
  }

  function savePng() {
    if (!nodes.length) return;
    const { text, w, h: hh } = standalone();
    const src = URL.createObjectURL(new Blob([text], { type: "image/svg+xml" }));
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = Math.round(w * 2);
      canvas.height = Math.round(hh * 2);
      const ctx = canvas.getContext("2d");
      ctx.scale(2, 2);
      ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--paper").trim();
      ctx.fillRect(0, 0, w, hh);
      ctx.drawImage(img, 0, 0, w, hh);
      URL.revokeObjectURL(src);
      canvas.toBlob((blob) => blob && offer(blob, `${fileName()}.png`), "image/png");
    };
    img.onerror = () => { URL.revokeObjectURL(src); showMessage(emptyState({ iconName: "alert", title: "Could not make a PNG", body: "The drawing could not be rasterised in this browser. The SVG download still works." })); };
    img.src = src;
  }

  function drawLegend() {
    clear(legend);
    const seen = new Map();
    for (const n of nodes) seen.set(typeOf(n.type).layer, layerFill(typeOf(n.type).layer));
    for (const l of LAYERS) {
      if (!seen.has(l)) continue;
      legend.append(h("span", null, h("span", { class: "swatch", style: { background: seen.get(l) } }), l));
    }
    legend.hidden = seen.size === 0;
  }

  scopeControls();
  drawCentre();
  drawPins();
  build();

  return () => {
    alive = false;
    stopWatchingPins();
    document.removeEventListener("amcli:select", onSelect);
    document.removeEventListener("pointerdown", outsideHits);
    window.removeEventListener("resize", moveHits);
    window.removeEventListener("scroll", moveHits, true);
    pz.destroy();
    bar.destroy();
  };
}

// Indices as ranges — `0-271`, `3,7-9,12` — so the whole model fits in a query
// string instead of a kilobyte of commas.
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
  return null;
}

function clamp(v, lo, hi) { return Math.max(lo, Math.min(hi, v)); }
