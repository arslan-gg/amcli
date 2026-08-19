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

import { h, clear, fmt, esc } from "./dom.js";
import { icon } from "./icons.js";

export function cls(...parts) {
  return parts.filter(Boolean).join(" ");
}

// "7 of 145" — the one count sentence, so it reads the same everywhere.
export function countLabel(shown, total) {
  return shown === total ? `${fmt(total)}` : `${fmt(shown)} of ${fmt(total)}`;
}

/* ---- buttons ------------------------------------------------------------ */

export function button({ label, iconName, title, onclick, variant, active, disabled, href, trailingIcon }) {
  const kids = [];
  if (iconName) kids.push(icon(iconName));
  if (label) kids.push(h("span", null, label));
  if (trailingIcon) kids.push(icon(trailingIcon, { class: "trail" }));
  const attrs = {
    class: cls("btn", variant && `btn-${variant}`, active && "is-active", !label && "btn-icon"),
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
      // The same class an icon-only button() gets, or a segment with no label
      // is --ctl-pad wider on each side than the square zoom buttons beside it.
      class: cls("btn", !o.label && "btn-icon", o.value === value && "is-active"),
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
  // The width goes in a custom property rather than on `width` itself, so the
  // stylesheet can override it: an inline width outranks every rule, and a
  // field that spilled into the overflow menu kept its narrow toolbar width in
  // the middle of a stack of full-width controls.
  const box = h("div", { class: "field", style: width ? { "--field-w": width } : null },
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
  const spill = h("div", { class: "toolbar-spill", hidden: true, role: "group", "aria-label": "More controls" });
  const more = h("button", {
    class: "btn btn-icon toolbar-more", type: "button", hidden: true,
    title: "More controls", "aria-label": "More controls", "aria-expanded": "false",
    onclick: (e) => {
      e.stopPropagation();
      const open = spill.hidden;
      if (open) { spill.hidden = false; anchorTo(spill, more); } else close();
      more.setAttribute("aria-expanded", String(open));
      more.classList.toggle("is-active", open);
    },
  }, icon("more"));
  const anchor = h("div", { class: "popover-anchor toolbar-more-wrap", hidden: true }, more, spill);
  function close() {
    spill.hidden = true;
    more.classList.remove("is-active");
    more.setAttribute("aria-expanded", "false");
  }
  const shut = (e) => { if (!anchor.contains(e.target)) close(); };
  document.addEventListener("pointerdown", shut);
  // Escape belongs to the topmost thing that is open, the way popover() has it:
  // without this it reached the shell's handler and cleared the selection while
  // the menu the reader was dismissing stayed put.
  const shutKey = (e) => { if (e.key === "Escape" && !spill.hidden) { e.stopPropagation(); close(); more.focus(); } };
  document.addEventListener("keydown", shutKey, true);

  rail.append(...items, anchor);
  bar.append(rail, h("div", { class: "toolbar-trail" }, (trailing || []).filter(Boolean)));

  function reflow() {
    // The panel is parked at fixed coordinates that a resize invalidates, and
    // its contents are about to move back into the bar. Shut it first, or
    // widening the window leaves an orphaned panel that reappears by itself.
    close();
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
  // Every page calls this from its unmount. A toolbar holds two document
  // listeners and an observer the page cannot see, and a bar left behind by a
  // navigation keeps its whole detached page alive through them.
  bar.destroy = () => {
    ro.disconnect();
    document.removeEventListener("pointerdown", shut);
    document.removeEventListener("keydown", shutKey, true);
  };
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
// How many hidden values are worth naming before the list stops informing.
const CHIP_CAP = 3;

export function filterBar(dimensions) {
  const bar = h("div", { class: "filterbar", role: "group", "aria-label": "Filters" });
  const redraws = [];

  for (const d of dimensions) {
    const trigger = h("button", { class: "btn btn-filter", type: "button" });
    const row = h("div", { class: "filter-row" });

    // No footer: every checkbox applies on the change, so there is nothing to
    // confirm, and the count it printed is the count the trigger prints
    // directly above the open panel. All/None stay — they act on the whole
    // list and there is no other route to them.
    const menu = popover(trigger, (panel, close) => {
      const values = d.values();
      const head = h("div", { class: "popover-head" },
        h("span", { class: "popover-title" }, d.label),
        h("span", { class: "spacer" }),
        button({ label: "All", variant: "quiet", onclick: () => { for (const v of values) d.hidden.delete(v.value); apply(); } }),
        button({ label: "None", variant: "quiet", onclick: () => { for (const v of values) d.hidden.add(v.value); apply(); } }));
      panel.append(head);
      const list = h("div", { class: "popover-list" });
      for (const v of values) {
        list.appendChild(h("label", { class: "opt", title: v.label, dataset: { value: v.value } },
          h("input", {
            type: "checkbox", checked: !d.hidden.has(v.value),
            onchange: (e) => { if (e.target.checked) d.hidden.delete(v.value); else d.hidden.add(v.value); apply(); },
          }),
          v.swatch ? h("span", { class: "swatch", style: { background: v.swatch } }) : null,
          h("span", { class: "ellipsis" }, v.label),
          h("span", { class: "opt-n" }, fmt(v.count))));
      }
      panel.append(list);
    });

    // Ticking one box redraws the panel, because the counts of every other
    // dimension move with it. What must survive that is where the reader was:
    // thirty-one element types in a fixed-height scroller means a rebuild that
    // scrolls to the top and drops focus to <body> costs you your place on
    // every single click.
    function apply() {
      const list = menu.querySelector(".popover-list");
      const at = list ? list.scrollTop : 0;
      const held = document.activeElement?.closest?.(".opt")?.dataset.value;
      d.onChange();
      redraw();
      menu.redraw();
      const after = menu.querySelector(".popover-list");
      if (after) after.scrollTop = at;
      if (held !== undefined) menu.querySelector(`.opt[data-value="${esc(held)}"] input`)?.focus();
    }

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
      // A chip is for undoing one exclusion at a glance, which is worth the
      // room while you have hidden two or three things and worthless once you
      // have hidden twenty-nine — a link from the statistics page hides every
      // type but one, and the rail filled with struck names that said nothing
      // the trigger's own "1 of 31" had not already said. Past a few, only the
      // way back is offered.
      if (off.length && off.length <= CHIP_CAP) {
        for (const v of off) {
          row.appendChild(chip({
            label: v.label, struck: true, removable: true,
            title: `Show ${v.label} again`,
            onRemove: () => { d.hidden.delete(v.value); apply(); },
          }));
        }
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
  // Sorting repaints the head, which destroys the very button that was
  // activated. Remember which column it was so paint() can hand focus back —
  // set on the click because a click does not focus a button on every
  // platform, and read again from wherever focus stands because one sort
  // repaints twice, once for the order and once for the rows.
  let refocusKey = null;

  function headCell(c) {
    const th = h("th", { class: cls(c.align === "right" && "right", c.width && "fit"), scope: "col", dataset: { key: c.key } });
    if (!c.sortable) { th.appendChild(h("span", null, c.label)); return th; }
    const on = sort.key === c.key;
    th.setAttribute("aria-sort", on ? (sort.dir === "asc" ? "ascending" : "descending") : "none");
    th.appendChild(h("button", {
      class: cls("th-btn", on && "is-sorted"), type: "button",
      title: `Sort by ${c.label}`,
      onclick: () => {
        refocusKey = c.key;
        if (sort.key === c.key) sort = { key: c.key, dir: sort.dir === "asc" ? "desc" : "asc" };
        else sort = { key: c.key, dir: c.numeric ? "desc" : "asc" };
        opts.onSort?.(sort);
      },
    }, h("span", null, c.label), on ? icon(sort.dir === "asc" ? "sort-asc" : "sort-desc", { class: "th-sort" }) : null));
    return th;
  }

  function paint() {
    const heldKey = refocusKey
      || (table.contains(document.activeElement) ? document.activeElement.closest("th")?.dataset.key : null);
    refocusKey = null;
    clear(table);
    // Fixed columns and a colgroup: with `auto` a nowrap cell sets its own
    // minimum and the table pushes past the pane, which is a sideways scroll
    // nobody asked for. A share of the width, and long text ellipsizes.
    //
    // The head is drawn before the empty branch and not inside it: a filter
    // that matches nothing used to take the whole sticky header band with it,
    // so the sort control you were using vanished and the page jumped when you
    // backspaced it into existence again.
    table.appendChild(h("colgroup", null, columns.map((c) =>
      h("col", { style: { width: c.width || "auto" } }))));
    table.appendChild(h("thead", null, h("tr", null, columns.map(headCell))));
    if (!rows.length) {
      note.textContent = "";
      tbody = null;
      table.appendChild(h("tbody", null, h("tr", null, h("td", { colspan: columns.length, class: "cell-empty" },
        emptyState({ title: emptyTitle, body: emptyBody })))));
    } else {
      tbody = h("tbody");
      const shown = rows.slice(0, cap);
      // Tab enters the table at the selected row, not always at the first one:
      // parking it on row 0 after every sort walks the reader back to the top.
      const focusAt = Math.max(0, shown.findIndex((r) => id(r) === selected));
      shown.forEach((row, i) => {
        const rid = id(row);
        const tr = h("tr", {
          class: cls(selected === rid && "is-selected"),
          tabindex: i === focusAt ? 0 : -1,
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
    if (heldKey) table.querySelector(`th[data-key="${esc(heldKey)}"] .th-btn`)?.focus();
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
      // One twisty's width per level, so a child's twisty stands exactly under
      // its parent's rather than a third of the way past it, and the whole
      // tree starts on the same inset as every other label in the rail.
      style: { paddingLeft: `calc(var(--sp-2) + ${n.depth} * var(--tree-indent))` },
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

// A short explanation of a control that has nothing in it yet — what it is for
// and how to put something there. Loud enough to be read once: the rail's
// ground is --surface-1, so a hint takes --surface-0 and stands up off it.
export function hint(text, iconName = "info") {
  return h("p", { class: "hint" }, icon(iconName), h("span", null, text));
}

export function emptyState({ iconName, title, body, actions }) {
  return h("div", { class: "empty" },
    iconName ? icon(iconName, { size: 24, class: "empty-icon" }) : null,
    h("p", { class: "empty-title" }, title),
    body ? h("p", { class: "empty-body" }, body) : null,
    actions?.length ? h("div", { class: "empty-actions" }, actions) : null);
}



export function section(title, ...children) {
  return h("section", { class: "sec" }, h("h2", { class: "caps" }, title), ...children);
}

export function kv(pairs) {
  return h("table", { class: "kv" }, h("tbody", null,
    pairs.filter(Boolean).map(([k, v]) => h("tr", null, h("th", { scope: "row" }, k), h("td", null, v)))));
}
