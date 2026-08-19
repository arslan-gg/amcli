// What the inspector shows: one concept in full — type, layer, folder, where
// it is drawn, everything it is connected to, its documentation and its
// properties.
//
// There is one of these, not two. The viewer used to render a concept twice —
// once in a 339px panel and once as an 820px page reached through a "maximize"
// button — from the same list-shaped markup, so the page bought line length
// and nothing else, and "minimize" guessed where you had come from.

import { h, clear, fmt, relLabel } from "../dom.js";
import { store, elem, rel, view, folderPath, detail, otherEnd } from "../store.js";
import { typeIcon, typeOf, accessLabel } from "../notation.js";
import { icon } from "../icons.js";
import { badge, button, section, kv, emptyState } from "../ui.js";
import { href } from "../router.js";
import { select } from "../app.js";

export function renderConcept(container, id) {
  clear(container);
  const found = store.byId.get(id);
  if (!found) {
    container.appendChild(emptyState({ iconName: "alert", title: "Not in the model", body: `Nothing has id ${id}.` }));
    return;
  }
  const box = h("div", { class: "detail" });
  container.appendChild(box);
  if (found.kind === "element") renderElement(box, found.i);
  else if (found.kind === "relation") renderRelation(box, found.i);
  else if (found.kind === "view") renderView(box, found.i);
}

/* ---- element ---------------------------------------------------------------- */

function renderElement(box, i) {
  const e = elem(i);
  const t = typeOf(e.type);

  box.appendChild(h("div", { class: "detail-head" },
    typeIcon(e.type, "type-icon boxed"),
    h("div", { class: "detail-title" },
      h("h1", null, e.name || "(unnamed)"),
      h("div", { class: "detail-meta" },
        badge({ label: e.type, solid: true }),
        badge({ label: t.layer, swatch: t.fill, title: `${t.layer} layer` })),
      h("p", { class: "id-line" }, folderPath(e.folder) || "(no folder)"))));

  box.appendChild(h("div", { class: "actions" },
    button({ iconName: "graph", label: "Open in graph", href: href("graph", null, { focus: e.id, depth: 1 }) })));

  const views = store.viewsOfElem[i];
  box.appendChild(section(`Drawn on ${countWord(views.length, "view")}`,
    views.length
      ? h("div", { class: "link-list" }, views.map((vi) =>
          h("a", { href: href("view", view(vi).id, { focus: e.id }), title: `Open ${view(vi).name} with this element outlined` },
            icon("view"), h("span", { class: "ellipsis" }, view(vi).name))))
      : h("p", { class: "subtle small" }, "No view — this element is in the model but not on a drawing.")));

  const out = store.out[i], inc = store.inc[i];
  if (!out.length && !inc.length) {
    box.appendChild(section("Relationships",
      h("p", { class: "subtle small" }, "None — this element is connected to nothing.")));
  } else {
    if (out.length) box.appendChild(relSection("Outgoing", out, i, "arrow-right"));
    if (inc.length) box.appendChild(relSection("Incoming", inc, i, "arrow-left"));
  }

  appendDocAndProps(box, e);
  box.appendChild(section("Identifier", h("p", { class: "id-line" }, e.id)));
}

// A row shows the relationship and where it goes; clicking follows it. One
// destination per row — traversing is what the list is for, and a
// relationship's own detail is a click on its line, or the Relationships
// table.
function relSection(title, rels, self, arrowIcon) {
  const rows = rels.map((ri) => {
    const r = rel(ri);
    const o = otherEnd(r, self);
    const other = o >= 0 ? elem(o) : null;
    const label = relLabel(r.type) + (r.type === "Access" && r.access !== null ? ` (${accessLabel(r.access)})` : "");
    return h("button", {
      class: "rel-row", type: "button",
      title: other ? `${label} → ${other.name}` : label,
      disabled: !other,
      onclick: () => other && select(other.id),
    },
      icon(arrowIcon, { class: "rel-arrow" }),
      h("span", { class: "rel-type" }, label),
      other ? typeIcon(other.type) : null,
      h("span", { class: "ellipsis" }, other ? other.name : "(another relationship)"));
  });
  return section(`${title} · ${fmt(rels.length)}`, h("div", { class: "link-list" }, rows));
}

/* ---- relationship ------------------------------------------------------------ */

function renderRelation(box, i) {
  const r = rel(i);
  const src = r.src >= 0 ? elem(r.src) : null;
  const tgt = r.tgt >= 0 ? elem(r.tgt) : null;

  box.appendChild(h("div", { class: "detail-head" },
    h("div", { class: "type-icon boxed" }, icon("relations")),
    h("div", { class: "detail-title" },
      h("h1", null, r.name || relLabel(r.type)),
      h("div", { class: "detail-meta" },
        badge({ label: relLabel(r.type), solid: true }),
        r.type === "Access" && r.access !== null ? badge({ label: accessLabel(r.access) }) : null,
        r.type === "Association" && r.directed ? badge({ label: "directed" }) : null),
      h("p", { class: "id-line" }, folderPath(r.folder) || "(no folder)"))));

  const end = (label, e, raw) => h("button", {
    class: "rel-row", type: "button", disabled: !e,
    title: e ? `Show ${e.name}` : "This end is missing from the model",
    onclick: () => e && select(e.id),
  },
    h("span", { class: "rel-type" }, label),
    e ? typeIcon(e.type) : null,
    h("span", { class: "ellipsis" }, e ? e.name : raw || "(missing)"));

  box.appendChild(section("Ends", h("div", { class: "link-list" },
    end("From", src, r.srcId), end("To", tgt, r.tgtId))));

  if (src) {
    box.appendChild(h("div", { class: "actions" },
      button({ iconName: "graph", label: "Open in graph", href: href("graph", null, { focus: src.id, depth: 1 }) })));
  }

  const views = store.viewsOfRel[i];
  box.appendChild(section(`Drawn on ${countWord(views.length, "view")}`,
    views.length
      ? h("div", { class: "link-list" }, views.map((vi) =>
          h("a", { href: href("view", view(vi).id, { focus: r.id }) },
            icon("view"), h("span", { class: "ellipsis" }, view(vi).name))))
      : h("p", { class: "subtle small" }, "No view.")));

  appendDocAndProps(box, r);
  box.appendChild(section("Identifier", h("p", { class: "id-line" }, r.id)));
}

/* ---- view --------------------------------------------------------------------- */

function renderView(box, i) {
  const v = view(i);
  box.appendChild(h("div", { class: "detail-head" },
    h("div", { class: "type-icon boxed" }, icon("view")),
    h("div", { class: "detail-title" },
      h("h1", null, v.name),
      h("div", { class: "detail-meta" },
        badge({ label: "View", solid: true }),
        v.viewpoint ? badge({ label: v.viewpoint }) : null),
      h("p", { class: "id-line" }, folderPath(v.folder)))));
  box.appendChild(h("div", { class: "actions" },
    button({ iconName: "view", label: "Open the drawing", variant: "primary", href: href("view", v.id) })));
  box.appendChild(section(`Holds ${countWord(v.elements.length, "element")}`,
    h("div", { class: "link-list" }, v.elements.slice(0, 200).map((ei) =>
      h("button", { class: "rel-row", type: "button", onclick: () => select(elem(ei).id) },
        typeIcon(elem(ei).type), h("span", { class: "ellipsis" }, elem(ei).name))))));
}

/* ---- shared ---------------------------------------------------------------------- */

// Documentation and properties are fetched when the inspector opens; the model
// blob only says whether there are any, so a megabyte of prose is not shipped
// to draw a table.
function appendDocAndProps(box, c) {
  if (!c.doc && !(c.props > 0)) return;
  const docSec = c.doc ? section("Documentation", h("p", { class: "subtle small" }, "Loading…")) : null;
  const propSec = c.props > 0 ? section("Properties", h("p", { class: "subtle small" }, "Loading…")) : null;
  if (docSec) box.appendChild(docSec);
  if (propSec) box.appendChild(propSec);
  detail(c.id).then((d) => {
    if (docSec) { clear(docSec); docSec.append(h("h2", { class: "sec-title" }, "Documentation"), h("div", { class: "doc" }, d.doc || "")); }
    if (propSec) { clear(propSec); propSec.append(h("h2", { class: "sec-title" }, "Properties"), kv(d.properties)); }
  });
}

function countWord(n, noun) {
  return `${fmt(n)} ${noun}${n === 1 ? "" : "s"}`;
}
