// Hash routes: #/views, #/view/ID, #/elements?type=X, #/element/ID,
// #/relations, #/relation/ID, #/graph?focus=ID&depth=2.

const handlers = new Set();

export function parse(hash = location.hash) {
  const raw = hash.replace(/^#\/?/, "");
  const [pathPart, queryPart = ""] = raw.split("?");
  const segs = pathPart.split("/").filter(Boolean).map(decodeURIComponent);
  const page = segs[0] || "views";
  return { page, id: segs[1] || null, params: new URLSearchParams(queryPart) };
}

export function href(page, id, params) {
  let h = `#/${page}`;
  if (id) h += `/${encodeURIComponent(id)}`;
  if (params) {
    const p = params instanceof URLSearchParams ? params : new URLSearchParams(params);
    for (const [k, v] of [...p.entries()]) if (v === "" || v === null || v === undefined) p.delete(k);
    const q = p.toString();
    if (q) h += `?${q}`;
  }
  return h;
}


// Change the query of the current route without adding history entries, for
// filters and toggles.
//
// The route handlers are deliberately *not* called: a page that changes its
// own filter has already redrawn the part that changed, and telling the router
// would tear the page down and build it again — losing whatever it holds that
// the URL does not, and asking the server a second time for the same answer.
export function replaceParams(params) {
  const r = parse();
  history.replaceState(null, "", href(r.page, r.id, params));
}

export function onRoute(fn) {
  handlers.add(fn);
  window.addEventListener("hashchange", () => fn(parse()));
  return () => handlers.delete(fn);
}
