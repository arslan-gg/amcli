// How big the model is, and of what — the same figures `amcli stats` prints.
//
// Two things the old page got wrong and this one does not: the cards were an
// `auto-fill` grid that left one card alone on a second row, and the two
// charts beside each other each normalised to their own tallest bar, so a
// 172-long bar sat level with a 79-long one and invited a comparison the
// lengths did not support. Charts that share a row now share a scale.

import { h, fmt, relLabel } from "../dom.js";
import { store, folderPath } from "../store.js";
import { typeOf } from "../notation.js";
import { href } from "../router.js";
import { toolbar, card, barChart, section, kv, emptyState } from "../ui.js";

const LAYERS = ["Strategy", "Business", "Application", "Technology", "Physical", "Motivation", "Implementation & Migration", "Other"];

export function mount(main) {
  const d = store.data;
  const page = h("div", { class: "page" });
  const body = h("div", { class: "page-body pad" });
  const bar = toolbar({ title: "Statistics", titleIcon: "stats", meta: d.model.name });
  page.append(bar, body);
  main.appendChild(page);

  let orphans = 0, undrawn = 0;
  d.elements.forEach((_, i) => {
    if (store.inc[i].length + store.out[i].length === 0) orphans++;
    if (store.viewsOfElem[i].length === 0) undrawn++;
  });

  body.appendChild(h("div", { class: "cards" },
    card({ value: fmt(d.elements.length), label: "Elements", href: href("elements") }),
    card({ value: fmt(d.relations.length), label: "Relationships", href: href("relations") }),
    card({ value: fmt(d.views.length), label: "Views", href: href("views") }),
    card({ value: fmt(d.folders.length), label: "Folders" }),
    card({ value: fmt(orphans), label: "Orphans", hint: "connected to nothing" }),
    card({ value: fmt(undrawn), label: "Undrawn", hint: "on no view" })));

  const byLayer = new Map(), byType = new Map(), byKind = new Map(), byFolder = new Map();
  const bump = (m, k) => m.set(k, (m.get(k) || 0) + 1);
  for (const e of d.elements) { bump(byLayer, e.layer); bump(byType, e.type); bump(byFolder, topFolder(e.folder)); }
  for (const r of d.relations) bump(byKind, r.type);

  // Every bar leads to the rows it counts. The collection page filters by what
  // is *hidden*, so a bar asks for its own value by naming every other one:
  // without that the link lands on the unfiltered list, which is what the nav
  // rail is for.
  const others = (all, mine) => all.filter((x) => x !== mine).join(",");
  const layerRows = LAYERS.filter((l) => byLayer.has(l)).map((l) => ({
    label: l, value: byLayer.get(l), swatch: layerFill(l),
    href: href("elements", null, { no_layer: others(LAYERS.filter((x) => byLayer.has(x)), l) }),
  }));
  const typeRows = [...byType.entries()].sort((a, b) => b[1] - a[1]).map(([t, n]) => ({
    label: t, value: n, swatch: typeOf(t).fill,
    href: href("elements", null, { no_type: others([...byType.keys()], t) }),
  }));
  const kindRows = [...byKind.entries()].sort((a, b) => b[1] - a[1]).map(([t, n]) => ({
    label: relLabel(t), value: n,
    href: href("relations", null, { no_kind: others([...byKind.keys()], t) }),
  }));
  const folderRows = [...byFolder.entries()].sort((a, b) => b[1] - a[1]).map(([f, n]) => ({
    label: f, value: n, href: href("elements", null, { folder: f === "(none)" ? "" : f }),
  }));

  // Elements by layer and by type count the same population, so they share a
  // scale and can be read against each other.
  const elementMax = Math.max(1, ...layerRows.map((r) => r.value), ...typeRows.map((r) => r.value));

  const grid = h("div", { class: "stats-grid" });
  grid.append(
    section("Elements by layer", barChart({ rows: layerRows, max: elementMax })),
    section("Elements by type", barChart({ rows: typeRows, max: elementMax })),
    section("Relationships by kind", barChart({ rows: kindRows })),
    section("Elements by top-level folder", barChart({ rows: folderRows, max: elementMax })));
  body.appendChild(grid);

  if (d.model.purpose) {
    body.appendChild(h("div", { class: "stats-prose" },
      section("Purpose", h("p", { class: "doc" }, d.model.purpose))));
  }
  if (d.model.properties.length) {
    body.appendChild(h("div", { class: "stats-prose" },
      section("Model properties", kv(d.model.properties))));
  }
  if (!d.elements.length && !d.views.length) {
    body.appendChild(emptyState({
      iconName: "info", title: "This model is empty",
      body: "Add elements with `amcli element add`, then draw one with `amcli view auto`.",
    }));
  }
  return () => bar.destroy();
}

function topFolder(fi) {
  const seg = folderPath(fi).split("/").filter(Boolean);
  return seg.length ? "/" + seg[0] : "(none)";
}

function layerFill(layer) {
  for (const t of Object.values(store.data.types)) if (t.layer === layer) return t.fill;
  return null;
}
