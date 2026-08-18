// How big the model is, and of what — the same figures `amcli stats` prints,
// as cards and bars.

import { h, fmt, relLabel } from "../dom.js";
import { store, folderPath } from "../store.js";
import { typeOf } from "../notation.js";
import { href } from "../router.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];
const LAYER_LABEL = {};

export function mount(main) {
  const d = store.data;
  const page = h("div", { class: "page" });
  const body = h("div", { class: "page-body pad" });
  page.append(h("div", { class: "page-head" }, h("h2", null, "Stats"), h("span", { class: "muted small mono ellipsis", title: d.model.path }, d.model.path)), body);
  main.appendChild(page);

  const orphans = [];
  d.elements.forEach((_, i) => { if (store.inc[i].length + store.out[i].length === 0) orphans.push(i); });
  const undrawn = [];
  d.elements.forEach((_, i) => { if (store.viewsOfElem[i].length === 0) undrawn.push(i); });

  body.appendChild(h("div", { class: "cards" },
    card(fmt(d.elements.length), "elements", href("elements")),
    card(fmt(d.relations.length), "relationships", href("relations")),
    card(fmt(d.views.length), "views", href("views")),
    card(fmt(d.folders.length), "folders"),
    card(fmt(orphans.length), "orphans — connected to nothing"),
    card(fmt(undrawn.length), "on no view"),
  ));

  const byLayer = {}, byType = {}, byRel = {}, byFolder = {};
  for (const e of d.elements) { byLayer[e.layer] = (byLayer[e.layer] || 0) + 1; byType[e.type] = (byType[e.type] || 0) + 1; byFolder[topFolder(e.folder)] = (byFolder[topFolder(e.folder)] || 0) + 1; }
  for (const r of d.relations) byRel[r.type] = (byRel[r.type] || 0) + 1;

  const grid = h("div", { class: "stats-grid" });
  grid.appendChild(section("By layer", LAYERS.filter((l) => byLayer[l]).map((l) => [LAYER_LABEL[l] || l, byLayer[l], href("elements", null, { layer: l }), l])));
  grid.appendChild(section("By relationship type", Object.entries(byRel).sort((a, b) => b[1] - a[1]).map(([t, n]) => [relLabel(t), n, href("relations", null, { type: t })])));
  grid.appendChild(section("By element type", Object.entries(byType).sort((a, b) => b[1] - a[1]).map(([t, n]) => [t, n, href("elements", null, { type: t }), null, t])));
  grid.appendChild(section("By top folder", Object.entries(byFolder).sort((a, b) => b[1] - a[1]).map(([f, n]) => [f, n, href("elements", null, { folder: f })])));
  body.appendChild(grid);

  if (d.model.purpose) body.appendChild(h("section", { style: { marginTop: "24px" } }, h("h3", null, "Purpose"), h("p", { class: "doc", style: { whiteSpace: "pre-wrap", marginTop: "6px" } }, d.model.purpose)));
  if (d.model.properties.length) {
    body.appendChild(h("section", { style: { marginTop: "24px", maxWidth: "600px" } }, h("h3", null, "Model properties"),
      h("table", { class: "kv", style: { marginTop: "6px" } }, h("tbody", null, d.model.properties.map(([k, v]) => h("tr", null, h("td", { class: "muted", style: { paddingRight: "16px" } }, k), h("td", null, v)))))));
  }
  return () => {};
}

function topFolder(fi) {
  const p = folderPath(fi);
  const seg = p.split("/").filter(Boolean);
  return seg.length ? "/" + seg[0] : "(none)";
}

function card(v, k, link) {
  const c = h("div", { class: "card" }, h("div", { class: "v" }, v), h("div", { class: "k" }, k));
  return link ? h("a", { href: link, style: { display: "block" } }, c) : c;
}

function section(title, rows) {
  const max = Math.max(1, ...rows.map((r) => r[1]));
  const bars = h("div", { class: "bars" });
  for (const [label, n, link, layer, type] of rows) {
    const fill = layer ? typeOf(sampleOfLayer(layer))?.fill : type ? typeOf(type).fill : null;
    bars.append(
      h("a", { href: link, class: "nowrap small" }, fill ? h("span", { class: "swatch", style: { background: fill, marginRight: "6px" } }) : null, label),
      h("div", { class: "bar" }, h("i", { style: { width: `${(100 * n) / max}%` } })),
      h("span", { class: "n" }, fmt(n)),
    );
  }
  return h("section", null, h("h3", { style: { marginBottom: "10px" } }, title), rows.length ? bars : h("p", { class: "muted small" }, "None."));
}

function sampleOfLayer(layer) {
  for (const [name, t] of Object.entries(store.data.types)) if (t.layer === layer) return name;
  return "";
}
