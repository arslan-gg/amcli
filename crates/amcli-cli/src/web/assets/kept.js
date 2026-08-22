// What the reader left behind, kept for the visit.
//
// Two things a page cannot rebuild from the file: how it was narrowed, and
// where its camera was pointing. Both are the reader's own work, and stepping
// to another page used to throw both away — a look at the graph and back, and
// a table narrowed to six rows was two hundred and seventy-two again from the
// top, a drawing zoomed into one corner was the whole sheet again.
//
// A page's filters are in the URL already, which is why a deep link works and
// why "back to the list" lands on the rows it was opened from. What has no URL
// to come from is the nav: `#/elements` written fresh carries nothing. So every
// page records the query it was last left under, and the nav asks here at the
// moment it is clicked — not when the link was built, because a href baked at
// build time is one filter out of date by the second letter typed.
//
// The camera stays out of the URL on purpose: it moves on every wheel notch,
// and a hash rewriting itself through a pan would bury the model under history
// entries. It is filed under the picture it was pointing at — a drawing's id,
// or the graph's centre, hops and direction — so coming back to the same
// picture comes back to the same corner of it, while asking for a different
// one is fitted afresh.
//
// Neither outlives the tab. This is where a reader was a minute ago, not a
// preference; the preferences worth keeping are in localStorage — the pins,
// the pane widths, the theme.

const params = new Map(); // page → the query it was last left under
const cameras = new Map(); // picture → {cx, cy, scale}

export function keepParams(page, p) {
  params.set(page, p);
}

export function lastParams(page) {
  return params.get(page) || {};
}

// A camera is a centre and a scale, never a viewBox: the pane is not the same
// width on the way back — an inspector dragged wider in between — and a
// viewBox replayed into a narrower pane is a different zoom.
export function keepCamera(picture, seat) {
  cameras.set(picture, seat);
}

export function lastCamera(picture) {
  return cameras.get(picture) || null;
}
