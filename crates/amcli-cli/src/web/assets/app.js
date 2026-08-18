// Boot: load the model, wire the shell (nav, search, theme, status, drawer),
// route the hash to a page, and keep the page in step with the file.

import { h, clear, fmt, relLabel, debounce } from "./dom.js";
import { store, load, subscribe, startPolling, search, elem, view, rel } from "./store.js";
import { parse, onRoute, href } from "./router.js";
import { typeIcon } from "./notation.js";
import { renderConcept } from "./pages/detail.js";
import * as views from "./pages/views.js";
import * as elements from "./pages/elements.js";
import * as relations from "./pages/relations.js";
import * as graph from "./pages/graph.js";
import * as stats from "./pages/stats.js";
import * as detail from "./pages/detail.js";

const PAGES = {
  views, view: views,
  elements, element: detail,
  relations, relation: detail,
  graph, stats,
};

const main = document.getElementById("main");
const panel = document.getElementById("details");
const panelBody = document.getElementById("details-body");
const statusEl = document.getElementById("status");

let unmount = () => {};
let selectedId = null;

// ---- theme ---------------------------------------------------------------
// Storage can be denied outright (a locked-down browser); the theme then
// simply follows the system for this visit.
const storage = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* not persisted */ } },
};
function applyTheme(t) {
  document.documentElement.dataset.theme = t;
  storage.set("amcli-theme", t);
}
applyTheme(storage.get("amcli-theme") || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"));
document.getElementById("theme").addEventListener("click", () => applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark"));

// ---- details panel ---------------------------------------------------------
export function select(id) {
  selectedId = id;
  const found = store.byId.get(id);
  if (!found) return;
  panel.hidden = false;
  document.getElementById("details-open").href = href(found.kind, id);
  renderConcept(panelBody, id);
}
export function closeDetails() { panel.hidden = true; selectedId = null; }

// Maximizing the panel opens the concept as a page; minimizing that page goes
// back to wherever the reader was — the view, the graph, the table — with the
// panel showing the same concept again.
let maximizedFrom = null;
document.getElementById("details-open").addEventListener("click", () => { maximizedFrom = location.hash; });
export function minimizeDetails(id) {
  const back = maximizedFrom && !/^#\/(element|relation)\//.test(maximizedFrom) ? maximizedFrom : "#/views";
  maximizedFrom = null;
  location.hash = back;
  select(id);
}
document.getElementById("details-close").addEventListener("click", closeDetails);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !panel.hidden && document.activeElement?.tagName !== "INPUT") closeDetails();
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); document.getElementById("search").focus(); }
});

// ---- status --------------------------------------------------------------
function setStatus(kind, text, title) {
  statusEl.className = `status ${kind}`;
  statusEl.querySelector(".status-text").textContent = text;
  statusEl.title = title || "";
}

// ---- search --------------------------------------------------------------
const searchInput = document.getElementById("search");
const searchResults = document.getElementById("search-results");
let activeHit = -1;
const runSearch = () => {
  const q = searchInput.value;
  clear(searchResults);
  activeHit = -1;
  if (!store.data || !q.trim()) { searchResults.hidden = true; return; }
  const hits = search(q, 12);
  const group = (title, items, render) => {
    if (!items.length) return;
    searchResults.appendChild(h("div", { class: "group" }, title));
    for (const it of items) searchResults.appendChild(render(it));
  };
  group("Elements", hits.elements, ({ i }) => h("a", { href: href("element", elem(i).id) }, typeIcon(elem(i).type), h("span", { class: "ellipsis" }, elem(i).name), h("span", { class: "type" }, elem(i).type)));
  group("Views", hits.views, ({ i }) => h("a", { href: href("view", view(i).id) }, "▣ ", h("span", { class: "ellipsis" }, view(i).name), h("span", { class: "type" }, `${view(i).elements.length} elements`)));
  group("Relationships", hits.relations, ({ i }) => h("a", { href: href("relation", rel(i).id) }, h("span", { class: "ellipsis" }, rel(i).name), h("span", { class: "type" }, relLabel(rel(i).type))));
  if (!searchResults.children.length) searchResults.appendChild(h("div", { class: "group" }, "No matches"));
  searchResults.hidden = false;
};
searchInput.addEventListener("input", debounce(runSearch, 60));
searchInput.addEventListener("focus", runSearch);
searchInput.addEventListener("blur", () => setTimeout(() => (searchResults.hidden = true), 150));
searchInput.addEventListener("keydown", (e) => {
  const links = [...searchResults.querySelectorAll("a")];
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    if (!links.length) return;
    activeHit = (activeHit + (e.key === "ArrowDown" ? 1 : -1) + links.length) % links.length;
    links.forEach((a, i) => a.classList.toggle("active", i === activeHit));
    links[activeHit].scrollIntoView({ block: "nearest" });
  } else if (e.key === "Enter") {
    const a = links[activeHit >= 0 ? activeHit : 0];
    if (a) { location.hash = a.getAttribute("href"); searchResults.hidden = true; searchInput.blur(); }
  } else if (e.key === "Escape") {
    searchResults.hidden = true; searchInput.blur();
  }
});
searchResults.addEventListener("click", () => { searchResults.hidden = true; });

// ---- routing -------------------------------------------------------------
function render(route) {
  if (!store.data) return;
  unmount();
  clear(main);
  const page = PAGES[route.page] || views;
  document.querySelectorAll("#nav a").forEach((a) => {
    const p = a.dataset.page;
    a.classList.toggle("active", p === route.page || (p === "views" && route.page === "view") || (p === "elements" && route.page === "element") || (p === "relations" && route.page === "relation"));
  });
  try {
    unmount = page.mount(main, route) || (() => {});
  } catch (e) {
    console.error(e);
    main.appendChild(h("div", { class: "empty" }, `This page failed to render: ${e.message}`));
    unmount = () => {};
  }
  // A page-level element or relation is its own details.
  if ((route.page === "element" || route.page === "relation") && route.id) closeDetails();
}

function refreshShell() {
  const d = store.data;
  const nameEl = document.getElementById("model-name");
  nameEl.textContent = d.model.name || "(unnamed model)";
  nameEl.title = d.model.path;
  document.title = `${d.model.name || "amcli"} — amcli`;
  const counts = { views: d.views.length, elements: d.elements.length, relations: d.relations.length };
  document.querySelectorAll("[data-count]").forEach((el) => (el.textContent = fmt(counts[el.dataset.count])));
}

subscribe((event) => {
  if (event === "model") { refreshShell(); }
  if (event === "changed") {
    setStatus("ok changed", "updated", `Model reloaded at ${new Date().toLocaleTimeString()}`);
    render(parse());
    if (selectedId && !panel.hidden) select(selectedId);
    setTimeout(() => setStatus("ok", "live", `Watching ${store.data.model.path}`), 2500);
  } else if (event === "status") {
    if (store.error) setStatus("error", "file invalid", `The model file no longer parses; showing the last good version.\n${store.error}`);
    else setStatus("ok", "live", `Watching ${store.data.model.path}`);
  } else if (event === "offline") {
    setStatus("error", "server gone", "amcli web is no longer answering — was it stopped?");
  }
});

onRoute(render);

load()
  .then(() => {
    setStatus("ok", "live", `Watching ${store.data.model.path}`);
    if (!location.hash) location.hash = "#/views";
    render(parse());
    startPolling(2000);
  })
  .catch((e) => {
    setStatus("error", "failed", e.message);
    main.appendChild(h("div", { class: "empty" }, `Could not load the model: ${e.message}`));
  });
