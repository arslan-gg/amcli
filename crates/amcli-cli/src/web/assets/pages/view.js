// One saved view: the very SVG the server renders from the file, so what is on
// screen is what Archi would draw. A click on a figure selects the concept it
// stands for; a double-click takes the graph there.

import { h, clear, fmt, esc } from "../dom.js";
import { store, view } from "../store.js";
import { href } from "../router.js";
import { attachPanZoom } from "../panzoom.js";
import { toolbar, button, iconButton, emptyState, badge } from "../ui.js";
import { select, selectedId, clearSelection } from "../app.js";
import { lastListParams } from "./collection.js";

export function mount(main, route) {
  const found = store.byId.get(route.id);
  if (!found || found.kind !== "view") {
    const bar = toolbar({ title: "View", titleIcon: "view", leading: backButton() });
    main.appendChild(h("div", { class: "page" }, bar,
      emptyState({ iconName: "view", title: "No such view", body: `Nothing in this model has id ${route.id}.` })));
    return () => bar.destroy();
  }
  return render(main, found.i, route.params.get("focus"));
}

function backButton() {
  return button({ iconName: "chevron-left", label: "Views", title: "Back to the list of views", href: href("views", null, lastListParams("views")) });
}

function render(main, vi, focus) {
  const v = view(vi);
  const page = h("div", { class: "page" });

  // There is no picker of every other view here. It was a flat `<select>` of
  // the whole model — the shape the collection page exists to be rid of — and
  // it could be neither searched nor filtered while the two routes that can
  // are a click away: the back button returns to the list you came from, and
  // ⌘K matches a view by name.
  const bar = toolbar({
    leading: backButton(),
    title: v.name,
    titleIcon: "view",
    meta: `${fmt(v.elements.length)} · ${fmt(v.relations.length)}`,
    controls: [v.viewpoint ? badge({ label: v.viewpoint, title: "Viewpoint" }) : null].filter(Boolean),
    trailing: [
      button({ iconName: "external", label: "SVG", title: "Open the SVG in a new tab", href: `/api/view/${encodeURIComponent(v.id)}.svg` }),
      button({ iconName: "download", label: "PNG", title: "Download as PNG at 2× resolution", href: `/api/view/${encodeURIComponent(v.id)}.png` }),
    ],
  });
  bar.querySelector(".toolbar-trail a[title^='Open the SVG']")?.setAttribute("target", "_blank");
  bar.querySelector(".toolbar-trail a[title^='Download']")?.setAttribute("download", `${safeName(v.name)}.png`);

  const canvas = h("div", { class: "canvas" });
  const hud = h("div", { class: "canvas-hud" });
  const msg = h("div", { class: "canvas-msg" }, "Rendering…");
  canvas.append(hud, msg);
  page.append(bar, canvas);
  main.appendChild(page);

  let pz = null;
  let alive = true;
  let marked = null;

  const url = `/api/view/${encodeURIComponent(v.id)}.svg?c=${encodeURIComponent(store.checksum)}`;
  fetch(url, { cache: "no-store" })
    .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`HTTP ${r.status}`))))
    .then((text) => {
      if (!alive) return;
      const doc = new DOMParser().parseFromString(text, "image/svg+xml");
      const svg = document.adoptNode(doc.documentElement);
      if (svg.nodeName.toLowerCase() !== "svg") throw new Error("the server did not return an SVG");
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
        iconButton("fit", "Fit to the window", () => pz.fit(box)),
        iconButton("plus", "Zoom in", () => pz.zoomIn()),
        iconButton("minus", "Zoom out", () => pz.zoomOut()));
      wire(svg, canvas, focus);
      if (!svg.querySelector("[data-concept]") && v.elements.length === 0) {
        canvas.appendChild(h("div", { class: "canvas-msg" },
          emptyState({ iconName: "view", title: "This view is empty", body: "Nothing has been added to it yet." })));
      }
    })
    .catch((e) => {
      if (!alive) return;
      clear(msg).appendChild(emptyState({ iconName: "alert", title: "Could not draw this view", body: e.message }));
    });

  function mark(g) {
    marked?.classList.remove("is-selected");
    marked = g;
    g?.classList.add("is-selected");
  }

  function wire(svg, canvas, focusId) {
    svg.addEventListener("click", (e) => {
      if (canvas.dataset.justDragged) return;
      const g = e.target.closest("[data-concept], [data-relationship]");
      if (!g) { mark(null); clearSelection(); return; }
      mark(g);
      select(g.dataset.concept || g.dataset.relationship);
    });
    svg.addEventListener("dblclick", (e) => {
      const g = e.target.closest("[data-concept]");
      if (!g) return;
      e.preventDefault();
      location.hash = href("graph", null, { focus: g.dataset.concept, depth: 1 });
    });
    const wanted = focusId || selectedId();
    if (wanted) {
      const g = svg.querySelector(`[data-concept="${esc(wanted)}"], [data-relationship="${esc(wanted)}"]`);
      if (g) { mark(g); select(wanted); }
    }
  }

  // Selecting elsewhere — the palette, the inspector's list of views — should
  // outline the figure here too.
  const onSelect = (e) => {
    const svg = canvas.querySelector("svg");
    if (!svg) return;
    const id = e.detail.id;
    mark(id ? svg.querySelector(`[data-concept="${esc(id)}"], [data-relationship="${esc(id)}"]`) : null);
  };
  document.addEventListener("amcli:select", onSelect);

  return () => {
    alive = false;
    pz?.destroy();
    bar.destroy();
    document.removeEventListener("amcli:select", onSelect);
  };
}

function safeName(name) {
  return (name || "view").replace(/[\\/:*?"<>|]+/g, "_").trim() || "view";
}
