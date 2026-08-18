// The model, once fetched, indexed for the pages; and the poll that notices
// when the file behind it changes.

const listeners = new Set();
const detailCache = new Map();

export const store = {
  data: null,
  checksum: "",
  loaded: 0,
  error: null,
  // Indexes, rebuilt on every load.
  byId: new Map(),        // id → {kind:"element"|"relation"|"view"|"folder", i}
  out: [],                // element index → [relation index]
  inc: [],
  viewsOfElem: [],        // element index → [view index]
  viewsOfRel: [],
  folderKids: [],         // folder index → [folder index]
  folderElems: [],        // folder index → [element index]
  folderRels: [],
  folderViews: [],
  roots: [],              // top-level folder indices
};

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function notify(event) {
  for (const fn of listeners) fn(event);
}

export async function load() {
  const r = await fetch("/api/model", { cache: "no-store" });
  if (!r.ok) throw new Error(`model: HTTP ${r.status}`);
  const data = await r.json();
  index(data);
  detailCache.clear();
  notify("model");
}

function index(data) {
  store.data = data;
  store.checksum = data.model.checksum;
  const byId = new Map();
  data.folders.forEach((f, i) => byId.set(f.id, { kind: "folder", i }));
  data.elements.forEach((e, i) => byId.set(e.id, { kind: "element", i }));
  data.relations.forEach((r, i) => byId.set(r.id, { kind: "relation", i }));
  data.views.forEach((v, i) => byId.set(v.id, { kind: "view", i }));
  store.byId = byId;

  const out = data.elements.map(() => []);
  const inc = data.elements.map(() => []);
  data.relations.forEach((r, i) => {
    if (r.src >= 0) out[r.src].push(i);
    if (r.tgt >= 0) inc[r.tgt].push(i);
  });
  store.out = out;
  store.inc = inc;

  const viewsOfElem = data.elements.map(() => []);
  const viewsOfRel = data.relations.map(() => []);
  data.views.forEach((v, vi) => {
    for (const e of v.elements) viewsOfElem[e].push(vi);
    for (const r of v.relations) viewsOfRel[r].push(vi);
  });
  store.viewsOfElem = viewsOfElem;
  store.viewsOfRel = viewsOfRel;

  const kids = data.folders.map(() => []);
  const roots = [];
  data.folders.forEach((f, i) => (f.parent === null ? roots : kids[f.parent]).push(i));
  store.folderKids = kids;
  store.roots = roots;
  store.folderElems = data.folders.map(() => []);
  store.folderRels = data.folders.map(() => []);
  store.folderViews = data.folders.map(() => []);
  data.elements.forEach((e, i) => { if (e.folder !== null) store.folderElems[e.folder].push(i); });
  data.relations.forEach((r, i) => { if (r.folder !== null) store.folderRels[r.folder].push(i); });
  data.views.forEach((v, i) => { if (v.folder !== null) store.folderViews[v.folder].push(i); });
}

// Documentation and properties, fetched when a detail opens and cached until
// the model reloads.
export async function detail(id) {
  if (detailCache.has(id)) return detailCache.get(id);
  const r = await fetch(`/api/concept/${encodeURIComponent(id)}`, { cache: "no-store" });
  if (!r.ok) return { doc: null, properties: [] };
  const d = await r.json();
  detailCache.set(id, d);
  return d;
}

// The server re-reads the file when it changes; the page asks every two
// seconds whether it has, and reloads itself when the checksum moves.
export function startPolling(intervalMs = 2000) {
  let busy = false;
  let skipped = 0;
  const tick = async () => {
    if (busy) return;
    // A hidden tab still follows the file, just five times more slowly.
    if (document.hidden && ++skipped % 5 !== 0) return;
    busy = true;
    try {
      const r = await fetch("/api/status", { cache: "no-store" });
      if (r.ok) {
        const st = await r.json();
        const changed = st.checksum !== store.checksum;
        store.error = st.error;
        store.loaded = st.loaded;
        if (changed) await load();
        notify(changed ? "changed" : "status");
      } else {
        notify("offline");
      }
    } catch {
      notify("offline");
    } finally {
      busy = false;
    }
  };
  const t = setInterval(tick, intervalMs);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) tick(); });
  return () => clearInterval(t);
}

// ---- lookups used by every page ------------------------------------------

export function elem(i) { return store.data.elements[i]; }
export function rel(i) { return store.data.relations[i]; }
export function view(i) { return store.data.views[i]; }
export function folder(i) { return store.data.folders[i]; }
export function typeInfo(type) { return store.data.types[type] || null; }
export function folderPath(i) { return i === null || i === undefined ? "" : store.data.folders[i].path; }

export function find(id) { return store.byId.get(id) || null; }

// The other end of a relationship, seen from element `e`.
export function otherEnd(r, e) { return r.src === e ? r.tgt : r.src; }

// Substring search over element, relation and view names, cheap enough to run
// on every keystroke for a model of a few thousand concepts.
export function search(q, limit = 30) {
  const needle = q.trim().toLowerCase();
  if (!needle) return { elements: [], views: [], relations: [] };
  const hits = { elements: [], views: [], relations: [] };
  const rank = (name) => {
    const n = name.toLowerCase();
    if (n === needle) return 0;
    if (n.startsWith(needle)) return 1;
    const at = n.indexOf(needle);
    return at < 0 ? -1 : 2 + at / 100;
  };
  const collect = (arr, key, out) => {
    for (let i = 0; i < arr.length; i++) {
      const r = rank(arr[i].name || "");
      if (r >= 0) out.push({ i, r });
    }
    out.sort((a, b) => a.r - b.r || (arr[a.i].name || "").localeCompare(arr[b.i].name || ""));
    out.length = Math.min(out.length, limit);
  };
  collect(store.data.elements, "name", hits.elements);
  collect(store.data.views, "name", hits.views);
  collect(store.data.relations, "name", hits.relations);
  return hits;
}
