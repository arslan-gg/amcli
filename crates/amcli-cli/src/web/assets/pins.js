// Which elements stay on the graph whatever the filters say.
//
// A pin is not the graph's private business, because the two places you decide
// to keep something are the drawing (shift-click a box) and the inspector
// (a button beside "Open in graph") — and the inspector is the shell's, shown
// over whichever page you are on. So the set lives here, and the graph reads it
// rather than owning it. The graph still writes it into the URL, because that
// is where a graph's shape is recorded; nowhere else has a URL to put it in.

const KEY = "amcli-pins";
const listeners = new Set();

// Kept across visits, and across pages. Pinning is a decision about the model,
// not about the graph that happens to be on screen: you make it in the
// inspector while reading a table, and it has to still be true when you open
// the graph an hour later. Storage can be denied outright, in which case the
// set simply lasts the visit.
const pinned = new Set(load());

function load() {
  try { return JSON.parse(localStorage.getItem(KEY) || "[]"); } catch { return []; }
}
function save() {
  try { localStorage.setItem(KEY, JSON.stringify([...pinned])); } catch { /* not persisted */ }
}

export function pins() {
  return [...pinned];
}

export function isPinned(id) {
  return pinned.has(id);
}

export function togglePin(id) {
  if (pinned.has(id)) pinned.delete(id); else pinned.add(id);
  notify();
  return pinned.has(id);
}

export function setPin(id, on) {
  if (on === pinned.has(id)) return;
  if (on) pinned.add(id); else pinned.delete(id);
  notify();
}

export function clearPins() {
  if (!pinned.size) return;
  pinned.clear();
  notify();
}

// Replacing the whole set — the graph does this from the URL when it mounts.
// Silent when nothing actually changes, so arriving at a graph whose pins are
// already the pins does not redraw it.
export function replacePins(ids) {
  const next = new Set(ids);
  if (next.size === pinned.size && [...next].every((id) => pinned.has(id))) return;
  pinned.clear();
  for (const id of next) pinned.add(id);
  notify();
}

export function onPins(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function notify() {
  save();
  for (const fn of [...listeners]) fn();
}
