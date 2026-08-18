// A small force simulation: every node repels every other, links pull their
// ends together, a weak force keeps the whole thing centred, and it cools
// until it stops. O(n²) a tick, which is fine for the few hundred nodes a
// person can read; the graph page caps what it shows well before that hurts.

export function createSimulation(nodes, links, opts = {}) {
  const o = {
    repulsion: 3200,       // strength of node-node repulsion
    linkDistance: 170,     // rest length of a link, centre to centre
    linkStrength: 0.08,
    gravity: 0.015,        // pull toward the origin
    collidePad: 14,        // clear space kept between boxes
    damping: 0.82,
    alphaDecay: 0.018,
    alphaMin: 0.004,
    maxSpeed: 40,
    ...opts,
  };
  let alpha = 1;
  let running = false;
  let raf = 0;
  const onTick = opts.onTick || (() => {});
  const onEnd = opts.onEnd || (() => {});

  // Scatter nodes that have no position yet in a loose spiral so the first
  // ticks have something to push against.
  nodes.forEach((n, i) => {
    if (n.x === undefined || Number.isNaN(n.x)) {
      const a = i * 2.39996, r = 30 + 12 * Math.sqrt(i);
      n.x = Math.cos(a) * r; n.y = Math.sin(a) * r;
    }
    n.vx = n.vx || 0; n.vy = n.vy || 0;
  });

  function step() {
    const n = nodes.length;
    for (let i = 0; i < n; i++) {
      const a = nodes[i];
      for (let j = i + 1; j < n; j++) {
        const b = nodes[j];
        let dx = b.x - a.x, dy = b.y - a.y;
        let d2 = dx * dx + dy * dy;
        if (d2 < 1) { dx = (Math.random() - 0.5) || 0.1; dy = (Math.random() - 0.5) || 0.1; d2 = dx * dx + dy * dy; }
        // Repel from the boxes' edges rather than their centres, so wide
        // boxes do not overlap where narrow ones would not.
        const pad = ((a.w || 100) + (b.w || 100)) / 4 + ((a.h || 40) + (b.h || 40)) / 4;
        const d = Math.sqrt(d2);
        const eff = Math.max(d - pad, 8);
        const f = (o.repulsion * alpha) / (eff * eff);
        const fx = (dx / d) * f, fy = (dy / d) * f;
        a.vx -= fx; a.vy -= fy;
        b.vx += fx; b.vy += fy;
      }
    }
    for (const l of links) {
      const a = l.source, b = l.target;
      if (a === b) continue;
      const dx = b.x - a.x, dy = b.y - a.y;
      const d = Math.sqrt(dx * dx + dy * dy) || 1;
      const rest = o.linkDistance + ((a.w || 0) + (b.w || 0)) / 4;
      const f = (d - rest) * o.linkStrength * alpha;
      const fx = (dx / d) * f, fy = (dy / d) * f;
      a.vx += fx; a.vy += fy;
      b.vx -= fx; b.vy -= fy;
    }
    for (const p of nodes) {
      p.vx -= p.x * o.gravity * alpha;
      p.vy -= p.y * o.gravity * alpha;
      p.vx *= o.damping; p.vy *= o.damping;
      const sp = Math.sqrt(p.vx * p.vx + p.vy * p.vy);
      if (sp > o.maxSpeed) { p.vx *= o.maxSpeed / sp; p.vy *= o.maxSpeed / sp; }
      if (p.fx !== undefined && p.fx !== null) { p.x = p.fx; p.y = p.fy; p.vx = 0; p.vy = 0; }
      else { p.x += p.vx; p.y += p.vy; }
    }
    // Boxes must not overlap, whatever the forces say: any pair that does is
    // pushed apart along the axis where the overlap is smallest.
    for (let pass = 0; pass < 2; pass++) {
      for (let i = 0; i < n; i++) {
        const a = nodes[i];
        for (let j = i + 1; j < n; j++) {
          const b = nodes[j];
          const ox = ((a.w || 100) + (b.w || 100)) / 2 + o.collidePad - Math.abs(b.x - a.x);
          if (ox <= 0) continue;
          const oy = ((a.h || 40) + (b.h || 40)) / 2 + o.collidePad - Math.abs(b.y - a.y);
          if (oy <= 0) continue;
          const aFixed = a.fx !== undefined && a.fx !== null;
          const bFixed = b.fx !== undefined && b.fx !== null;
          if (aFixed && bFixed) continue;
          const share = aFixed ? 0 : bFixed ? 1 : 0.5;
          if (ox < oy) {
            const s = (b.x >= a.x ? 1 : -1) * ox;
            a.x -= s * share; b.x += s * (1 - share);
          } else {
            const s = (b.y >= a.y ? 1 : -1) * oy;
            a.y -= s * share; b.y += s * (1 - share);
          }
        }
      }
    }
    alpha -= alpha * o.alphaDecay;
  }

  function loop() {
    if (!running) return;
    // A few steps per frame keeps it snappy without starving the paint.
    for (let i = 0; i < 3 && alpha > o.alphaMin; i++) step();
    onTick();
    if (alpha <= o.alphaMin) { running = false; onEnd(); return; }
    raf = requestAnimationFrame(loop);
  }

  return {
    start() { if (!running) { running = true; raf = requestAnimationFrame(loop); } },
    reheat(a = 0.6) { alpha = Math.max(alpha, a); this.start(); },
    stop() { running = false; cancelAnimationFrame(raf); },
    get alpha() { return alpha; },
    get running() { return running; },
    // Run to rest synchronously — for a first layout before the user sees it.
    settle(maxSteps = 300) { for (let i = 0; i < maxSteps && alpha > o.alphaMin; i++) step(); onTick(); },
  };
}
