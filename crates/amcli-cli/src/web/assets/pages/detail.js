// One concept in full: type, layer, folder, documentation, properties, every
// relationship in and out, and the views it is drawn on. Used by the drawer
// (a click on a diagram or a graph node) and by the #/element and #/relation
// pages, which are the same thing with more room.

import { h, clear, relLabel } from "../dom.js";
import { store, elem, rel, view, folderPath, detail, otherEnd } from "../store.js";
import { typeIcon, typeOf, accessLabel } from "../notation.js";
import { href } from "../router.js";
import { minimizeDetails } from "../app.js";

export function mount(main, route) {
  const found = store.byId.get(route.id);
  const wrap = h("div", { class: "page" });
  const body = h("div", { class: "page-body pad" });
  wrap.appendChild(h("div", { class: "page-head" },
    h("button", { class: "btn sm", title: "Minimize — back to where you were, with this in the details panel", onclick: () => minimizeDetails(route.id) }, "⤡ Minimize"),
    h("h2", null, route.page === "relation" ? "Relationship" : "Element")));
  wrap.appendChild(body);
  main.appendChild(wrap);
  if (!found || (found.kind !== "element" && found.kind !== "relation")) {
    body.appendChild(h("div", { class: "empty" }, `Nothing in the model has id ${route.id}`));
    return () => {};
  }
  const box = h("div", { class: "detail", style: { maxWidth: "820px" } });
  body.appendChild(box);
  if (found.kind === "element") renderElement(box, found.i, { heading: "h1" });
  else renderRelation(box, found.i, { heading: "h1" });
  return () => {};
}

export function renderConcept(container, id, opts = {}) {
  clear(container);
  const found = store.byId.get(id);
  if (!found) { container.appendChild(h("div", { class: "empty" }, "Not in the model")); return; }
  const box = h("div", { class: "detail" });
  container.appendChild(box);
  if (found.kind === "element") renderElement(box, found.i, opts);
  else if (found.kind === "relation") renderRelation(box, found.i, opts);
  else if (found.kind === "view") renderViewSummary(box, found.i);
}

function layerBadge(type) {
  const t = typeOf(type);
  return h("span", { class: "badge" }, h("span", { class: "swatch", style: { background: t.fill } }), t.layer);
}

export function renderElement(box, i, opts = {}) {
  const e = elem(i);
  const H = opts.heading || "h2";
  box.appendChild(h("div", { class: "detail-head" },
    typeIcon(e.type, "type-icon boxed"),
    h("div", { style: { minWidth: 0 } },
      h(H, null, e.name || h("span", { class: "muted" }, "(unnamed)")),
      h("div", { class: "detail-meta" },
        h("span", { class: "badge solid" }, e.type),
        layerBadge(e.type),
        e.folder !== null ? h("span", { class: "muted small mono" }, folderPath(e.folder)) : null,
      ),
    ),
  ));

  box.appendChild(h("div", { class: "actions" },
    h("a", { class: "btn sm", href: href("graph", null, { focus: e.id, depth: 1 }) }, "Open in graph"),
  ));

  // Where it is drawn, and what it is connected to, come first: they are what
  // a reader clicks on next.
  const views = store.viewsOfElem[i];
  box.appendChild(h("section", null,
    h("h3", null, `Views (${views.length})`),
    views.length
      ? h("div", { class: "link-list" }, views.map((vi) => h("a", { href: href("view", view(vi).id, { focus: e.id }) }, "▣ ", h("span", { class: "ellipsis" }, view(vi).name))))
      : h("p", { class: "muted small" }, "Drawn on no view."),
  ));

  const out = store.out[i], inc = store.inc[i];
  if (out.length + inc.length === 0) {
    box.appendChild(h("section", null, h("h3", null, "Relationships"), h("p", { class: "muted small" }, "None — this element is connected to nothing.")));
  } else {
    if (out.length) box.appendChild(relSection("Outgoing", out, i, "→"));
    if (inc.length) box.appendChild(relSection("Incoming", inc, i, "←"));
  }

  appendDocAndProps(box, e);
  box.appendChild(h("p", { class: "muted small mono", style: { wordBreak: "break-all" } }, e.id));
}

// Documentation and properties are fetched when the panel opens; the blob
// only says whether there are any.
function appendDocAndProps(box, c) {
  const docSec = h("section", null, h("h3", null, "Documentation"), h("p", { class: "muted small" }, "…"));
  const propSec = h("section", null, h("h3", null, "Properties"), h("p", { class: "muted small" }, "…"));
  if (c.doc) box.appendChild(docSec);
  if (c.props > 0) box.appendChild(propSec);
  if (!c.doc && !(c.props > 0)) return;
  detail(c.id).then((d) => {
    if (c.doc) { clear(docSec); docSec.append(h("h3", null, "Documentation"), h("div", { class: "doc" }, d.doc || "")); }
    if (c.props > 0) {
      clear(propSec);
      propSec.append(h("h3", null, "Properties"),
        h("table", { class: "kv" }, h("tbody", null, d.properties.map(([k, v]) => h("tr", null, h("td", null, k), h("td", null, v))))));
    }
  });
}

function relSection(title, rels, self, arrow) {
  const rows = rels.map((ri) => {
    const r = rel(ri);
    const o = otherEnd(r, self);
    const other = o >= 0 ? elem(o) : null;
    return h("a", { href: href("relation", r.id), title: r.name || "" },
      other ? typeIcon(other.type) : h("span"),
      h("span", { class: "rtype" }, relLabel(r.type), r.type === "Access" && r.access !== null ? ` (${accessLabel(r.access)})` : ""),
      h("span", { class: "ellipsis" }, h("span", { class: "arrow" }, arrow, " "), other ? other.name : h("span", { class: "muted" }, "(another relationship)")),
    );
  });
  return h("section", null, h("h3", null, `${title} (${rels.length})`), h("div", { class: "rel-list" }, rows));
}

export function renderRelation(box, i, opts = {}) {
  const r = rel(i);
  const H = opts.heading || "h2";
  const src = r.src >= 0 ? elem(r.src) : null;
  const tgt = r.tgt >= 0 ? elem(r.tgt) : null;
  const title = r.name ? `${relLabel(r.type)}: ${r.name}` : relLabel(r.type);
  box.appendChild(h("div", { class: "detail-head" },
    h("div", { style: { minWidth: 0 } },
      h(H, null, title),
      h("div", { class: "detail-meta" },
        h("span", { class: "badge solid" }, relLabel(r.type)),
        r.type === "Access" && r.access !== null ? h("span", { class: "badge" }, accessLabel(r.access)) : null,
        r.type === "Association" && r.directed ? h("span", { class: "badge" }, "directed") : null,
        r.folder !== null ? h("span", { class: "muted small mono" }, folderPath(r.folder)) : null,
      ),
    ),
  ));
  const endRow = (label, e, rawId) => h("tr", null,
    h("td", null, label),
    h("td", null, e
      ? h("a", { href: href("element", e.id), style: { display: "inline-flex", alignItems: "center" } }, typeIcon(e.type), e.name, h("span", { class: "muted small", style: { marginLeft: "6px" } }, e.type))
      : h("span", { class: "muted mono small" }, rawId || "(missing)")),
  );
  box.appendChild(h("section", null,
    h("table", { class: "kv" }, h("tbody", null, endRow("Source", src, r.srcId), endRow("Target", tgt, r.tgtId))),
  ));
  box.appendChild(h("div", { class: "actions" },
    src ? h("a", { class: "btn sm", href: href("graph", null, { focus: src.id, depth: 1 }) }, "Open in graph") : null,
  ));
  const views = store.viewsOfRel[i];
  box.appendChild(h("section", null,
    h("h3", null, `Views (${views.length})`),
    views.length
      ? h("div", { class: "link-list" }, views.map((vi) => h("a", { href: href("view", view(vi).id, { focus: r.id }) }, "▣ ", h("span", { class: "ellipsis" }, view(vi).name))))
      : h("p", { class: "muted small" }, "Drawn on no view."),
  ));
  appendDocAndProps(box, r);
  box.appendChild(h("p", { class: "muted small mono", style: { wordBreak: "break-all" } }, r.id));
}

function renderViewSummary(box, i) {
  const v = view(i);
  box.appendChild(h("div", { class: "detail-head" }, h("div", null, h("h2", null, v.name), h("div", { class: "detail-meta" }, h("span", { class: "badge solid" }, "View"), v.viewpoint ? h("span", { class: "badge" }, v.viewpoint) : null))));
  box.appendChild(h("div", { class: "actions" }, h("a", { class: "btn sm primary", href: href("view", v.id) }, "Open view")));
  box.appendChild(h("section", null, h("h3", null, `Elements (${v.elements.length})`),
    h("div", { class: "link-list" }, v.elements.map((ei) => h("a", { href: href("element", elem(ei).id) }, typeIcon(elem(ei).type), elem(ei).name)))));
}
