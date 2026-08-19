// Tiny DOM helpers. Model strings only ever go through textContent or
// attribute values — never innerHTML — because names come from the file.

const SVG = "http://www.w3.org/2000/svg";

export function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  apply(el, attrs);
  append(el, children);
  return el;
}

export function s(tag, attrs, ...children) {
  const el = document.createElementNS(SVG, tag);
  apply(el, attrs);
  append(el, children);
  return el;
}

function apply(el, attrs) {
  if (!attrs) return;
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") el.setAttribute("class", v);
    // Property by property rather than Object.assign, because a custom
    // property is not a member of CSSStyleDeclaration: `--field-w` assigned
    // that way is silently dropped, and setProperty is the only route in.
    else if (k === "style" && typeof v === "object") {
      for (const [p, val] of Object.entries(v)) {
        if (p.startsWith("--")) el.style.setProperty(p, val);
        else el.style[p] = val;
      }
    }
    else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v);
    else if (k === "dataset") Object.assign(el.dataset, v);
    else if (v === true) el.setAttribute(k, "");
    else el.setAttribute(k, v);
  }
}

function append(el, children) {
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    el.appendChild(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}

// A model string inside a selector. Names come from the file, and one quote in
// one of them makes querySelector throw rather than miss.
export function esc(v) {
  return window.CSS?.escape ? CSS.escape(v) : String(v).replace(/["\\]/g, "\\$&");
}

export function clear(el) {
  while (el.firstChild) el.removeChild(el.firstChild);
  return el;
}

export function fmt(n) {
  return new Intl.NumberFormat().format(n);
}

// A short label for a relationship type name: "AccessRelationship" → "Access".
export function relLabel(type) {
  return type.replace(/Relationship$/, "");
}

