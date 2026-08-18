// Pan and zoom by rewriting the SVG viewBox, so whatever the SVG contains —
// a rendered view straight from the server, or a graph we drew — is left
// untouched. Wheel zooms about the cursor; dragging the background pans.

export function attachPanZoom(svg, container, opts = {}) {
  const state = { x: 0, y: 0, w: 100, h: 100, content: null };
  let dragging = null;

  const apply = () => svg.setAttribute("viewBox", `${state.x} ${state.y} ${state.w} ${state.h}`);

  // Fit `box` ({x,y,w,h}) into the container with a margin.
  const fit = (box, pad = 24) => {
    if (!box || box.w <= 0 || box.h <= 0) return;
    state.content = box;
    const cw = container.clientWidth || 800, ch = container.clientHeight || 600;
    const scale = Math.min(cw / (box.w + 2 * pad), ch / (box.h + 2 * pad), opts.maxFitScale || 1.5);
    state.w = cw / scale;
    state.h = ch / scale;
    state.x = box.x + box.w / 2 - state.w / 2;
    state.y = box.y + box.h / 2 - state.h / 2;
    apply();
  };

  const actual = () => {
    const cw = container.clientWidth, ch = container.clientHeight;
    const cx = state.x + state.w / 2, cy = state.y + state.h / 2;
    state.w = cw; state.h = ch;
    state.x = cx - cw / 2; state.y = cy - ch / 2;
    apply();
  };

  const zoomAt = (clientX, clientY, factor) => {
    const r = container.getBoundingClientRect();
    const px = state.x + ((clientX - r.left) / r.width) * state.w;
    const py = state.y + ((clientY - r.top) / r.height) * state.h;
    const nw = Math.min(Math.max(state.w * factor, 40), 200000);
    const nh = state.h * (nw / state.w);
    state.x = px - ((clientX - r.left) / r.width) * nw;
    state.y = py - ((clientY - r.top) / r.height) * nh;
    state.w = nw; state.h = nh;
    apply();
  };

  const onWheel = (e) => {
    e.preventDefault();
    const factor = Math.exp((e.deltaMode === 1 ? e.deltaY * 20 : e.deltaY) * 0.0015);
    zoomAt(e.clientX, e.clientY, factor);
  };
  const onDown = (e) => {
    if (e.button !== 0) return;
    if (opts.isNodeTarget && opts.isNodeTarget(e.target)) return;
    e.preventDefault();
    dragging = { sx: e.clientX, sy: e.clientY, x: state.x, y: state.y, moved: false, id: e.pointerId };
    container.classList.add("dragging");
  };
  const onMove = (e) => {
    if (!dragging) return;
    const r = container.getBoundingClientRect();
    const dx = ((e.clientX - dragging.sx) / r.width) * state.w;
    const dy = ((e.clientY - dragging.sy) / r.height) * state.h;
    if (!dragging.moved && Math.abs(e.clientX - dragging.sx) + Math.abs(e.clientY - dragging.sy) > 3) {
      dragging.moved = true;
      // Capture only once this is a drag: capturing on the way down would
      // swallow the click a figure is waiting for.
      container.setPointerCapture?.(dragging.id);
      document.body.classList.add("dragging");
    }
    if (!dragging.moved) return;
    state.x = dragging.x - dx;
    state.y = dragging.y - dy;
    apply();
  };
  const onUp = () => {
    if (dragging?.moved) container.dataset.justDragged = "1";
    else delete container.dataset.justDragged;
    dragging = null;
    container.classList.remove("dragging");
    document.body.classList.remove("dragging");
  };
  // When the pane changes size, keep the scale and the centre: a panel
  // opening beside the drawing must not throw away the zoom the reader chose.
  let lastSize = { w: container.clientWidth, h: container.clientHeight };
  const onResize = () => {
    const cw = container.clientWidth, ch = container.clientHeight;
    if (!cw || !ch || !lastSize.w || !lastSize.h) { lastSize = { w: cw, h: ch }; return; }
    const scale = lastSize.w / state.w;
    const cx = state.x + state.w / 2, cy = state.y + state.h / 2;
    state.w = cw / scale; state.h = ch / scale;
    state.x = cx - state.w / 2; state.y = cy - state.h / 2;
    lastSize = { w: cw, h: ch };
    apply();
  };

  container.addEventListener("wheel", onWheel, { passive: false });
  container.addEventListener("pointerdown", onDown);
  container.addEventListener("pointermove", onMove);
  container.addEventListener("pointerup", onUp);
  container.addEventListener("pointercancel", onUp);
  const ro = new ResizeObserver(onResize);
  ro.observe(container);

  // Client → SVG user coordinates, for the graph's node dragging.
  const toSvg = (clientX, clientY) => {
    const r = container.getBoundingClientRect();
    return { x: state.x + ((clientX - r.left) / r.width) * state.w, y: state.y + ((clientY - r.top) / r.height) * state.h };
  };

  return {
    fit, actual, toSvg, apply,
    zoomIn: () => { const r = container.getBoundingClientRect(); zoomAt(r.left + r.width / 2, r.top + r.height / 2, 0.8); },
    zoomOut: () => { const r = container.getBoundingClientRect(); zoomAt(r.left + r.width / 2, r.top + r.height / 2, 1.25); },
    get viewBox() { return { ...state }; },
    destroy() {
      container.removeEventListener("wheel", onWheel);
      container.removeEventListener("pointerdown", onDown);
      container.removeEventListener("pointermove", onMove);
      container.removeEventListener("pointerup", onUp);
      container.removeEventListener("pointercancel", onUp);
      ro.disconnect();
    },
  };
}
