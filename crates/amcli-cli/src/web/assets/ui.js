// The primitives. Every control on every page is one of these.
//
// The rule this file exists to enforce: a widget is written once. The viewer
// used to carry three sortable table headers with three different ideas about
// which columns sort descending first, three filter→sort→cap→render pipelines
// and four ways to spell "N of M". Three copies drift three ways, and no
// amount of care at the call site fixes that — there has to be one thing to
// be consistent with.
//
// Nothing here knows what an ArchiMate model is. Notation lives in
// `notation.js`, chrome icons in `icons.js`, and anything a page needs to say
// about a concept it says by handing a render function in.

import { h, clear, fmt } from "./dom.js";
import { icon } from "./icons.js";

export function cls(...parts) {
  return parts.filter(Boolean).join(" ");
}

// "7 of 145" — the one count sentence, so it reads the same everywhere.
export function countLabel(shown, total) {
  return shown === total ? `${fmt(total)}` : `${fmt(shown)} of ${fmt(total)}`;
}

/* ---- buttons ------------------------------------------------------------ */

export function button({ label, iconName, title, onclick, variant, size, active, disabled, href, trailingIcon }) {
  const kids = [];
  if (iconName) kids.push(icon(iconName));
  if (label) kids.push(h("span", null, label));
  if (trailingIcon) kids.push(icon(trailingIcon, { class: "trail" }));
  const attrs = {
    class: cls("btn", variant && `btn-${variant}`, size === "lg" && "btn-lg", active && "is-active", !label && "btn-icon"),
    title: title || (label ? null : undefined),
    onclick,
    "aria-pressed": active === undefined ? null : String(!!active),
  };
  if (href) return h("a", { ...attrs, href, "aria-pressed": null }, kids);
  return h("button", { ...attrs, type: "button", disabled: disabled || null }, kids);
}

export function iconButton(iconName, title, onclick, opts = {}) {
  return h("button", {
    class: cls("btn btn-icon", opts.variant && `btn-${opts.variant}`, opts.active && "is-active"),
    type: "button",
    title,
    "aria-label": title,
    "aria-pressed": opts.active === undefined ? null : String(!!opts.active),
    onclick,
  }, icon(iconName));
}

// Two or three mutually exclusive choices, as one control.
export function segmented(options, value, onchange) {
  const box = h("div", { class: "segmented", role: "group" });
  for (const o of options) {
    box.appendChild(h("button", {
      class: cls("btn", o.value === value && "is-active"),
      type: "button",
      title: o.title || o.label,
      "aria-pressed": String(o.value === value),
      dataset: { value: o.value },
      onclick: () => {
        if (o.value === value) return;
        box.querySelectorAll(".btn").forEach((b) => {
          const on = b.dataset.value === o.value;
          b.classList.toggle("is-active", on);
          b.setAttribute("aria-pressed", String(on));
        });
        value = o.value;
        onchange(o.value);
      },
    }, o.iconName ? icon(o.iconName) : null, o.label ? h("span", null, o.label) : null));
  }
  return box;
}

export function chip({ label, count, active, onclick, title, swatch, removable, onRemove, struck }) {
  const kids = [];
  if (swatch) kids.push(h("span", { class: "swatch", style: { background: swatch } }));
  kids.push(h("span", { class: "chip-label" }, label));
  if (count !== undefined && count !== null) kids.push(h("span", { class: "chip-count" }, fmt(count)));
  if (removable) kids.push(icon("close", { class: "chip-x" }));
  return h("button", {
    class: cls("chip", active && "is-active", struck && "is-off"),
    type: "button",
    title: title || label,
    "aria-pressed": active === undefined ? null : String(!!active),
    onclick: removable ? onRemove : onclick,
  }, kids);
}

export function badge({ label, solid, swatch, iconNode, title }) {
  return h("span", { class: cls("badge", solid && "is-solid"), title: title || null },
    swatch ? h("span", { class: "swatch", style: { background: swatch } }) : null,
    iconNode || null,
    h("span", { class: "ellipsis" }, label));
}

/* ---- fields -------------------------------------------------------------- */

export function searchField({ value, placeholder, oninput, width, hint }) {
  const input = h("input", {
    class: "field-input", type: "search", value: value || "", placeholder,
    autocomplete: "off", spellcheck: "false", oninput: (e) => oninput(e.target.value),
  });
  const box = h("div", { class: "field", style: width ? { width } : null },
    icon("search", { class: "field-icon" }), input,
    hint ? h("kbd", null, hint) : null);
  box.input = input;
  return box;
}

export function selectField({ value, options, onchange, title, width }) {
  const sel = h("select", {
    class: "field-select", title, style: width ? { width } : null,
    onchange: (e) => onchange(e.target.value),
  });
  for (const o of options) {
    sel.appendChild(h("option", { value: o.value, selected: o.value === value }, o.label));
  }
  return sel;
}

/* ---- popover ------------------------------------------------------------- */

// Park a floating panel under whatever opened it, in viewport coordinates.
//
// Every container that holds a trigger — the rail, the toolbar — clips its own
// overflow, which is how the old search results ended up sliced 32px short of
// their own right edge. A fixed panel is not clipped by any of them; it only
// has to stay inside the window.
export function anchorTo(panel, trigger) {
  const r = trigger.getBoundingClientRect();
  const gap = 4;
  const w = panel.offsetWidth || r.width;
  const hgt = panel.offsetHeight || 0;
  const left = Math.max(gap, Math.min(r.left, window.innerWidth - w - gap));
  const below = r.bottom + gap;
  const top = below + hgt > window.innerHeight - gap && r.top - gap - hgt > gap
    ? r.top - gap - hgt
    : below;
  panel.style.left = `${Math.round(left)}px`;
  panel.style.top = `${Math.round(top)}px`;
  panel.style.maxHeight = `${Math.round(window.innerHeight - top - gap)}px`;
}

// One popover, so every menu opens, closes, traps Escape and gives focus back
// the same way. `fill(body, close)` draws the contents each time it opens.
export function popover(trigger, fill, opts = {}) {
  const panel = h("div", { class: cls("popover", opts.wide && "is-wide"), hidden: true, role: "dialog" });
  const wrap = h("div", { class: "popover-anchor" }, trigger, panel);

  const onOutside = (e) => { if (!wrap.contains(e.target) && !panel.contains(e.target)) close(); };
  const onKey = (e) => { if (e.key === "Escape") { e.stopPropagation(); close(true); } };
  const reposition = () => { if (!panel.hidden) anchorTo(panel, trigger); };

  function close(refocus) {
    if (panel.hidden) return;
    panel.hidden = true;
    trigger.setAttribute("aria-expanded", "false");
    document.removeEventListener("pointerdown", onOutside);
    document.removeEventListener("keydown", onKey, true);
    window.removeEventListener("resize", reposition);
    window.removeEventListener("scroll", reposition, true);
    if (refocus) trigger.focus();
  }
  function open() {
    clear(panel);
    fill(panel, close);
    panel.hidden = false;
    anchorTo(panel, trigger);
    trigger.setAttribute("aria-expanded", "true");
    document.addEventListener("pointerdown", onOutside);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    panel.querySelector("input, button, [tabindex]")?.focus();
  }

  trigger.setAttribute("aria-haspopup", "dialog");
  trigger.setAttribute("aria-expanded", "false");
  trigger.addEventListener("click", (e) => { e.stopPropagation(); panel.hidden ? open() : close(); });

  wrap.close = close;
  wrap.redraw = () => { if (!panel.hidden) { clear(panel); fill(panel, close); anchorTo(panel, trigger); } };
  return wrap;
}

/* ---- toolbar ------------------------------------------------------------- */

// The one page-header anatomy: title · meta · controls · spacer · trailing.
// It scrolls its controls rather than wrapping, because a bar whose height
// depends on how many layers the model has is a bar that moves the page
// underneath the reader.
export function toolbar({ title, titleIcon, meta, controls, trailing, leading }) {
  const bar = h("div", { class: "toolbar" });
  if (leading) bar.appendChild(h("div", { class: "toolbar-lead" }, leading));
  if (title) {
    bar.appendChild(h("div", { class: "toolbar-title" },
      titleIcon ? icon(titleIcon) : null,
      h("h1", null, title),
      meta ? h("span", { class: "toolbar-meta ellipsis" }, meta) : null));
  }
  const rail = h("div", { class: "toolbar-controls" });
  const items = (controls || []).filter(Boolean);

  // What does not fit goes into a menu, not off the side. A bar you have to
  // scroll sideways hides controls while looking full, and a control you
  // cannot see is a control you do not have.
  const spill = h("div", { class: "toolbar-spill", hidden: true });
  const more = h("button", {
    class: "btn btn-icon toolbar-more", type: "button", hidden: true,
    title: "More controls", "aria-label": "More controls", "aria-expanded": "false",
    onclick: (e) => {
      e.stopPropagation();
      const open = spill.hidden;
      spill.hidden = !open;
      if (open) anchorTo(spill, more);
      more.setAttribute("aria-expanded", String(open));
      more.classList.toggle("is-active", open);
    },
  }, icon("more"));
  const anchor = h("div", { class: "popover-anchor toolbar-more-wrap", hidden: true }, more, spill);
  const shut = (e) => { if (!anchor.contains(e.target)) { spill.hidden = true; more.classList.remove("is-active"); more.setAttribute("aria-expanded", "false"); } };
  document.addEventListener("pointerdown", shut);

  rail.append(...items, anchor);
  bar.append(rail, h("div", { class: "toolbar-trail" }, (trailing || []).filter(Boolean)));

  function reflow() {
    for (const c of items) rail.insertBefore(c, anchor);
    anchor.hidden = true;
    more.hidden = true;
    if (rail.scrollWidth <= rail.clientWidth) return;
    anchor.hidden = false;
    more.hidden = false;
    for (let i = items.length - 1; i >= 0 && rail.scrollWidth > rail.clientWidth; i--) {
      spill.prepend(items[i]);
    }
    if (!spill.children.length) { anchor.hidden = true; more.hidden = true; }
  }
  const ro = new ResizeObserver(reflow);
  ro.observe(rail);
  requestAnimationFrame(reflow);

  bar.controls = rail;
  bar.destroy = () => { ro.disconnect(); document.removeEventListener("pointerdown", shut); };
  return bar;
}

/* ---- filter bar ---------------------------------------------------------- */

// One filter idiom, whatever the dimension: a menu that says how much of the
// dimension is showing, and a chip for each value being hidden — because what
// is hidden is the short list and the one worth a click. Thirty-one element
// types as thirty-one chips wrapped over three lines and filtered nothing.
//
// dimension = { key, label, noun, values: () => [{value,label,count,swatch}],
//               hidden: Set, onChange: () => void }
export function filterBar(dimensions) {
  const bar = h("div", { class: "filterbar", role: "group", "aria-label": "Filters" });
  const redraws = [];

  for (const d of dimensions) {
    const trigger = h("button", { class: "btn btn-filter", type: "button" });
    const row = h("div", { class: "filter-row" });

    const menu = popover(trigger, (panel, close) => {
      const values = d.values();
      const head = h("div", { class: "popover-head" },
        h("span", { class: "popover-title" }, d.label),
        h("span", { class: "spacer" }),
        button({ label: "All", size: "sm", onclick: () => { for (const v of values) d.hidden.delete(v.value); apply(); } }),
        button({ label: "None", size: "sm", onclick: () => { for (const v of values) d.hidden.add(v.value); apply(); } }));
      panel.append(head);
      const list = h("div", { class: "popover-list" });
      for (const v of values) {
        list.appendChild(h("label", { class: "opt", title: v.label },
          h("input", {
            type: "checkbox", checked: !d.hidden.has(v.value),
            onchange: (e) => { if (e.target.checked) d.hidden.delete(v.value); else d.hidden.add(v.value); apply(); },
          }),
          v.swatch ? h("span", { class: "swatch", style: { background: v.swatch } }) : null,
          h("span", { class: "ellipsis" }, v.label),
          h("span", { class: "opt-n" }, fmt(v.count))));
      }
      panel.append(list);
      panel.appendChild(h("div", { class: "popover-foot" },
        h("span", { class: "muted" }, `${fmt(values.length - values.filter((v) => d.hidden.has(v.value)).length)} of ${fmt(values.length)} showing`),
        h("span", { class: "spacer" }),
        button({ label: "Done", variant: "primary", onclick: () => close(true) })));
      panel.close = close;
    });

    function apply() { d.onChange(); redraw(); menu.redraw(); }

    function redraw() {
      const values = d.values();
      const off = values.filter((v) => d.hidden.has(v.value));
      trigger.textContent = "";
      trigger.append(
        document.createTextNode(off.length ? `${fmt(values.length - off.length)} of ${fmt(values.length)}` : `All ${fmt(values.length)}`),
        icon("chevron-down", { class: "trail" }));
      trigger.title = `Choose which ${d.noun} to show`;
      trigger.classList.toggle("is-active", off.length > 0);
      clear(row);
      row.appendChild(menu);
      for (const v of off) {
        row.appendChild(chip({
          label: v.label, struck: true, removable: true,
          title: `Show ${v.label} again`,
          onRemove: () => { d.hidden.delete(v.value); apply(); },
        }));
      }
      if (off.length > 1) {
        row.appendChild(chip({ label: `Show all ${d.noun}`, onclick: () => { d.hidden.clear(); apply(); } }));
      }
    }

    bar.append(h("span", { class: "filter-key" }, d.label), row);
    redraws.push(redraw);
    redraw();
  }

  bar.redraw = () => redraws.forEach((f) => f());
  return bar;
}

/* ---- data table ---------------------------------------------------------- */

// Sorting, sticky heads, selection, the cap note, `aria-sort` and arrow keys —
// once, for every table in the viewer.
//
// columns = [{ key, label, align, sortable, numeric, render(row) }]
export function dataTable(opts) {
  const {
    columns, id, onSelect, onOpen, cap = 1000,
    emptyTitle = "Nothing to show", emptyBody = "",
  } = opts;

  const table = h("table", { class: "grid" });
  const note = h("div", { class: "grid-note" });
  const scroll = h("div", { class: "grid-scroll" }, table, note);
  let rows = opts.rows || [];
  let sort = opts.sort || { key: columns.find((c) => c.sortable)?.key, dir: "asc" };
  let selected = opts.selected || null;
  let tbody = null;

  function headCell(c) {
    const th = h("th", { class: cls(c.align === "right" && "right", c.width && "fit"), scope: "col" });
    if (!c.sortable) { th.appendChild(h("span", null, c.label)); return th; }
    const on = sort.key === c.key;
    th.setAttribute("aria-sort", on ? (sort.dir === "asc" ? "ascending" : "descending") : "none");
    th.appendChild(h("button", {
      class: cls("th-btn", on && "is-sorted"), type: "button",
      title: `Sort by ${c.label}`,
      onclick: () => {
        if (sort.key === c.key) sort = { key: c.key, dir: sort.dir === "asc" ? "desc" : "asc" };
        else sort = { key: c.key, dir: c.numeric ? "desc" : "asc" };
        opts.onSort?.(sort);
      },
    }, h("span", null, c.label), on ? icon(sort.dir === "asc" ? "sort-asc" : "sort-desc", { class: "th-sort" }) : null));
    return th;
  }

  function paint() {
    clear(table);
    if (!rows.length) {
      note.textContent = "";
      table.appendChild(h("tbody", null, h("tr", null, h("td", { colspan: columns.length, class: "cell-empty" },
        emptyState({ title: emptyTitle, body: emptyBody })))));
      return;
    }
    // Fixed columns and a colgroup: with `auto` a nowrap cell sets its own
    // minimum and the table pushes past the pane, which is a sideways scroll
    // nobody asked for. A share of the width, and long text ellipsizes.
    table.appendChild(h("colgroup", null, columns.map((c) =>
      h("col", { style: { width: c.width || "auto" } }))));
    table.appendChild(h("thead", null, h("tr", null, columns.map(headCell))));
    tbody = h("tbody");
    const shown = rows.slice(0, cap);
    shown.forEach((row, i) => {
      const rid = id(row);
      const tr = h("tr", {
        class: cls(selected === rid && "is-selected"),
        tabindex: i === 0 ? 0 : -1,
        dataset: { id: rid },
        onclick: () => pick(tr, row),
        ondblclick: () => onOpen?.(row),
      }, columns.map((c) => h("td", { class: cls(c.align === "right" && "right", c.cls) }, c.render(row))));
      tbody.appendChild(tr);
    });
    table.appendChild(tbody);
    note.textContent = rows.length > cap
      ? `Showing the first ${fmt(cap)} of ${fmt(rows.length)}. Narrow the filter to see the rest.`
      : "";
  }

  function pick(tr, row) {
    tbody?.querySelector("tr.is-selected")?.classList.remove("is-selected");
    tr.classList.add("is-selected");
    selected = id(row);
    roveTo(tr);
    onSelect?.(row);
  }

  function roveTo(tr) {
    tbody?.querySelectorAll("tr[tabindex='0']").forEach((x) => x.setAttribute("tabindex", "-1"));
    tr.setAttribute("tabindex", "0");
  }

  table.addEventListener("keydown", (e) => {
    const tr = e.target.closest("tr");
    if (!tr || !tbody?.contains(tr)) return;
    const all = [...tbody.children];
    const at = all.indexOf(tr);
    let next = null;
    if (e.key === "ArrowDown") next = all[Math.min(at + 1, all.length - 1)];
    else if (e.key === "ArrowUp") next = all[Math.max(at - 1, 0)];
    else if (e.key === "Home") next = all[0];
    else if (e.key === "End") next = all[all.length - 1];
    else if (e.key === "PageDown") next = all[Math.min(at + 12, all.length - 1)];
    else if (e.key === "PageUp") next = all[Math.max(at - 12, 0)];
    else if (e.key === "Enter") { e.preventDefault(); onOpen?.(rows[at]); return; }
    else if (e.key === " ") { e.preventDefault(); pick(tr, rows[at]); return; }
    else return;
    e.preventDefault();
    if (!next) return;
    roveTo(next);
    next.focus();
    pick(next, rows[all.indexOf(next)]);
    next.scrollIntoView({ block: "nearest" });
  });

  paint();
  return {
    el: scroll,
    set rows(next) { rows = next; paint(); },
    get rows() { return rows; },
    set sort(next) { sort = next; paint(); },
    // Selecting from somewhere else — the palette, a figure on a drawing, a
    // deep link — has to bring the row into view, or the table silently
    // disagrees with the inspector beside it.
    setSelected(rid, opts = {}) {
      selected = rid;
      let hit = null;
      tbody?.querySelectorAll("tr").forEach((tr) => {
        const on = tr.dataset.id === rid;
        tr.classList.toggle("is-selected", on);
        if (on) hit = tr;
      });
      if (hit && opts.reveal !== false) hit.scrollIntoView({ block: "nearest" });
    },
    focusFirst() { tbody?.querySelector("tr")?.focus(); },
  };
}

/* ---- tree ---------------------------------------------------------------- */

// A hierarchical dimension is always a tree, and always this one. A folder
// path was a 41-entry flat `<select>` on one page and a tree on another; the
// same shape has to look the same wherever it is asked for.
//
// nodes = [{ key, label, count, depth, hasKids, title }] — already flattened
// by the caller, which knows what is collapsed.
export function tree({ nodes, active, onPick, onToggle, isOpen, label }) {
  const box = h("div", { class: "tree", role: "tree", "aria-label": label || "Folders" });
  nodes.forEach((n, i) => {
    const on = n.key === active;
    const row = h("div", {
      class: cls("tree-row", on && "is-active", !n.count && "is-empty"),
      role: "treeitem",
      "aria-selected": String(on),
      "aria-expanded": n.hasKids ? String(isOpen(n.key)) : null,
      "aria-level": String(n.depth + 1),
      tabindex: on || (!active && i === 0) ? 0 : -1,
      title: n.title || n.label,
      style: { paddingLeft: `calc(${n.depth} * var(--sp-3))` },
      dataset: { key: n.key },
      onclick: () => onPick(n.key),
    },
      n.hasKids
        ? h("button", {
            class: "twisty", type: "button", tabindex: -1,
            "aria-label": isOpen(n.key) ? `Collapse ${n.label}` : `Expand ${n.label}`,
            onclick: (e) => { e.stopPropagation(); onToggle(n.key); },
          }, icon(isOpen(n.key) ? "chevron-down" : "chevron-right"))
        : h("span", { class: "twisty" }),
      h("span", { class: "tree-label ellipsis" }, n.label),
      h("span", { class: "tree-n" }, fmt(n.count)));
    box.appendChild(row);
  });

  box.addEventListener("keydown", (e) => {
    const row = e.target.closest(".tree-row");
    if (!row) return;
    const all = [...box.children];
    const at = all.indexOf(row);
    const key = row.dataset.key;
    let next = null;
    if (e.key === "ArrowDown") next = all[Math.min(at + 1, all.length - 1)];
    else if (e.key === "ArrowUp") next = all[Math.max(at - 1, 0)];
    else if (e.key === "Home") next = all[0];
    else if (e.key === "End") next = all[all.length - 1];
    else if (e.key === "ArrowRight") { if (!isOpen(key)) { e.preventDefault(); onToggle(key); } return; }
    else if (e.key === "ArrowLeft") { if (isOpen(key)) { e.preventDefault(); onToggle(key); } return; }
    else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onPick(key); return; }
    else return;
    e.preventDefault();
    if (!next) return;
    all.forEach((r) => r.setAttribute("tabindex", "-1"));
    next.setAttribute("tabindex", "0");
    next.focus();
  });

  return box;
}

/* ---- odds and ends -------------------------------------------------------- */

export function emptyState({ iconName, title, body, actions }) {
  return h("div", { class: "empty" },
    iconName ? icon(iconName, { size: 24, class: "empty-icon" }) : null,
    h("p", { class: "empty-title" }, title),
    body ? h("p", { class: "empty-body" }, body) : null,
    actions?.length ? h("div", { class: "empty-actions" }, actions) : null);
}

export function card({ value, label, href, hint }) {
  const inner = h("div", { class: "card" },
    h("span", { class: "card-value" }, value),
    h("span", { class: "card-label" }, label),
    hint ? h("span", { class: "card-hint" }, hint) : null);
  return href ? h("a", { class: "card-link", href, title: `Open ${label}` }, inner) : inner;
}

// A labelled bar chart. `max` is passed in so two charts side by side can
// share a scale instead of each normalising to its own tallest bar.
export function barChart({ rows, max }) {
  const top = max || Math.max(1, ...rows.map((r) => r.value));
  const box = h("div", { class: "bars" });
  for (const r of rows) {
    const label = r.href
      ? h("a", { class: "bar-label ellipsis", href: r.href, title: r.label }, r.swatch ? h("span", { class: "swatch", style: { background: r.swatch } }) : null, r.label)
      : h("span", { class: "bar-label ellipsis", title: r.label }, r.swatch ? h("span", { class: "swatch", style: { background: r.swatch } }) : null, r.label);
    box.append(label,
      h("div", { class: "bar", role: "img", "aria-label": `${r.label}: ${fmt(r.value)}` },
        h("i", { style: { width: `${(100 * r.value) / top}%` } })),
      h("span", { class: "bar-n" }, fmt(r.value)));
  }
  return box;
}

export function section(title, ...children) {
  return h("section", { class: "sec" }, h("h2", { class: "sec-title" }, title), ...children);
}

export function kv(pairs) {
  return h("table", { class: "kv" }, h("tbody", null,
    pairs.filter(Boolean).map(([k, v]) => h("tr", null, h("th", { scope: "row" }, k), h("td", null, v)))));
}
