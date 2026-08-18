# How amcli draws a view

`amcli view render` draws the geometry the model stores: every figure on the
bounds Archi recorded, every connection on the polyline Archi computes. It does
**not** promise pixel identity with Archi, which is not achievable in
principle — Archi's default view font is the platform system font, so its own
export differs between macOS and Windows.

`amcli view auto` and `amcli view layout` are the interesting part: they lay a
view out from the graph, and from nothing else.

## The graph decides, not the layer and not the arrows

The ArchiMate layer is deliberately not consulted: most relationships in a
real model run *within* a layer, so ranking by layer puts them in one row and
turns each into a horizontal line through whatever sits between its ends.
Nor, in the end, does the direction of the arrows decide. What a drawing is
for is being read, and a line through a box or across another line is what
stops that; so several layerings are tried per connected component and the
least tangled drawing is kept, and the arrows point down only when that costs
nothing.

The first candidate follows the arrows — ranks by network simplex, so the rows
are as few as the edges allow and each edge as short as it can be. The others
ignore direction and grow rings out from a hub, each ring's groups going to
whichever side of the hub most of their parents are on. That is what puts a
hub's fan half above it and half below, a value stream along one row under the
lifecycle that composes it, and two capabilities serving the same crowd on
opposite sides of it — none of which a drawing that must point every arrow
down can do, and the difference between a stack of crossings and none. Two
boxes in one row joined by an edge are drawn side by side, and the ordering
keeps them so.

## Ordering, placement, folding

Each connected component is laid out on its own and the components are packed
side by side. Within a component, nodes are ordered within a row by median and
sifting to cut crossings, and given x by Brandes and Köpf — aligned into
blocks with a median neighbour, a corridor and the boxes at both ends of its
long edge first so the whole edge is one column, and of a fan the middle slot
first so a hub stands over the middle of what it links to.

A rank too wide to read — wider than a screen and four times wider than the
drawing is tall — is folded onto several lines, and the fold nests: the outer
boxes on the near line, the inner on the far, so an inner box's edge crosses
the near line between the outer boxes and never through one.

## Straight lines, kept off the boxes

Every edge is one straight line, centre to centre; no bendpoints are ever
written. The layout keeps lines off boxes by where it puts the boxes: a long
edge reserves a corridor in each row it crosses, sequenced where the line will
run and as wide as its slant, and the boxes pack around it; every edge is kept
off the boxes of the rows it ends in by the row gaps, which are computed so
that each line has dropped clear of the row band before it reaches a
neighbour.

On a graph that admits a clean drawing the result has no edge through a box
and no two edges crossing; tests assert that for a fan, for two hubs sharing a
crowd, for a chain under a hub and for a folded fan, and a sweep of four
hundred random graphs asserts that no edge between neighbouring rows ever cuts
a box and that the share of long edges drawn through a rank they skip stays
under a bound. That share is not zero — a slanted line across a crowded rank
sometimes has no seat for its corridor that does not cost more crossings than
it saves — and it is reported.

What this cannot do is make a non-planar graph planar. Three hubs sharing
three boxes cross at least once however they are drawn, and in rows they cross
more. That is the model, not the drawing.

## Sizes, algorithms, exports

Boxes are sized to their labels: the width a name wraps into two lines at,
from the stock 120 up to 264, and taller only when it must be. `view layout`
writes sizes back along with positions and straightens every connection, so a
relaid view is redrawn, not just shuffled.

`--layout auto` is layered unless a grid would be both squarer and no more
tangled — crossings plus edges through boxes, which a grid can never route
around — which in practice means the fallback is for edgeless sets.
`--layout layered` and `--layout grid` force one or the other.

`export mermaid` and `export dot` re-lay-out, so they are for a quick look in a
chat window rather than for reproducing a diagram someone drew.
