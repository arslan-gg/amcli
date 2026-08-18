//! Placing concepts on a new view.
//!
//! Rows come from the graph and from nothing else. The ArchiMate layer is
//! deliberately not consulted: most relationships in a real model run
//! *within* a layer, so ranking by layer puts them all in one row and turns
//! each into a horizontal line slicing through whatever sits between its
//! ends. Nor, in the end, does the direction of the arrows decide: what the
//! drawing is for is being read, and a line through a box or across another
//! line is what stops that. So several layerings are tried and the least
//! tangled drawing is kept, and the arrows point down only when that costs
//! nothing.
//!
//! This is Sugiyama's method as Gansner et al. describe it for `dot`, with
//! the layering step opened up. Seven things do the work.
//!
//! **Layering, several ways.** The first candidate follows the arrows:
//! longest path shoves every node as high as it can go and anchors every
//! sink wherever the longest chain into it ends, so network simplex then
//! moves whole subtrees to minimise total edge length and most edges span
//! one row. The others ignore direction and grow rings out from a hub — the
//! three of highest degree, one at a time — putting each ring's groups on
//! whichever side of the hub most of their parents are, or the narrower
//! side when that is a tie. That is what puts a hub's fan half above it and
//! half below, a chain hung from a hub along one row beneath it, and two
//! hubs sharing a crowd on opposite sides of it — none of which a layering
//! that must point every arrow down can do. Each candidate is drawn all the
//! way and the drawings compared: crossings plus lines through boxes, then
//! long edges, then rows, then area, then the directed one.
//!
//! **Edges along a row.** Two boxes in one row joined by an edge are drawn
//! side by side and the edge is a short line between them. Such edges form
//! paths — the layering refuses a third at any box, and a cycle — and the
//! ordering moves a path as one group, in order, so nothing is ever put
//! between two of its boxes.
//!
//! **Folding a rank that will not fit.** A hundred motivation elements two
//! ranks deep give layering nothing to stack, and the rank runs off the side
//! of any screen. Such a rank is folded onto several lines. The lines nest:
//! the outer boxes on the line nearest the rows the rank's edges lead to,
//! the inner boxes on the far lines, so an inner box's edge crosses the
//! near line where the box would have stood — between the outer boxes, and
//! never through one — and the corridors of all the inner boxes lie in one
//! block that the outer boxes flank. Nothing narrower than a screen is
//! folded at all.
//!
//! **Ordering by median, transpose and sifting.** Within each row, groups
//! are put at the median of what they link to — one with nothing to link
//! to holds its place — adjacent pairs are then swapped wherever that
//! uncrosses something; and once that stalls, each group is tried at every
//! position in its row. The three together get within a few per cent of what
//! a far more expensive search finds on real views.
//!
//! **Placing by Brandes and Köpf.** Slots are aligned into blocks with a
//! median neighbour in the row above or below — a corridor with its corridor
//! neighbour first, and the box at the end of a long edge with the corridor,
//! so the whole edge is one column; and of a fan that wants the same
//! neighbour, its middle slot first, so a hub stands over the middle of its
//! fan and not at its end — and the blocks packed as tight as the boxes
//! allow, four ways; the layout with the fewest slanted long edges is kept
//! whole. Aligned slots share an x exactly, and a long edge whose ends both
//! sit on its column cannot pass through a box.
//!
//! **Straight lines, and lanes to keep them clear.** Every edge is one
//! straight line from centre to centre; there are no bendpoints. An edge
//! crossing several rows reserves a corridor in each — sequenced where the
//! line will run and as wide as its slant across the row — and the boxes
//! pack around it. An edge is kept off the boxes of the rows it ends in by
//! the row gaps, which are chosen so that every line has dropped clear of
//! the row band before it reaches a neighbour: computed rather than tuned,
//! and per gap. What is left is a slanted long edge across a crowded rank
//! whose corridor the ordering could not seat where the line runs without
//! more crossings than it saves; those are drawn through, and counted.
//!
//! What this cannot do is make a non-planar graph planar. Three hubs
//! sharing three boxes cross at least once however they are drawn, and in
//! rows they cross more; a rank of thirty requirements over thirty drivers
//! with seventy-five edges between them has a crossing number in the low
//! hundreds however it is ordered. That is the model, not the drawing.
//!
//! Everything is deterministic by construction: no randomness, no seeds, no
//! iteration over a hash map, ties broken by `(name, id)`, all coordinates
//! integers snapped to the grid.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::geometry::{Pt, Rect};

/// Grid the output snaps to. Archi's own bounds are integers anyway, so float
/// drift would only add diff noise.
pub const GRID: i32 = 12;

const HGAP: i32 = 48;
const VGAP: i32 = 72;
/// Width reserved in a row for an edge passing through it.
const DUMMY_W: i32 = 12;

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Algorithm {
    /// Layered, unless that degenerates — see [`MAX_WIDTH_RATIO`].
    #[default]
    Auto,
    /// Rows from the graph, every edge one straight line.
    Sugiyama,
    /// Sorted into a square grid. Never pretty, never fails.
    Grid,
}

impl Algorithm {
    pub fn parse(s: &str) -> Option<Algorithm> {
        Some(match s {
            "auto" => Algorithm::Auto,
            "sugiyama" | "layered" => Algorithm::Sugiyama,
            "grid" => Algorithm::Grid,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Algorithm::Auto => "auto",
            Algorithm::Sugiyama => "layered",
            Algorithm::Grid => "grid",
        }
    }

    /// What a caller may pass, for the error that says they did not.
    pub const NAMES: &'static str = "auto, layered (or sugiyama), grid";
}

/// How many times wider than tall a drawing may get before it is judged
/// unreadable.
///
/// A wide, shallow graph — a hundred motivation elements two ranks deep — gives
/// layering nothing to stack, so left to itself it puts them all in one row and
/// the view comes out thousands of pixels wide and a few hundred tall. Nothing
/// about that is incorrect; it simply cannot be read, printed or scrolled.
///
/// This bound is used twice: the layering folds an over-wide rank so the
/// drawing stays inside it, and `auto` checks it afterwards in case a rank
/// could not be folded.
///
/// The test is deliberately one-sided. Tall and narrow is what a correctly
/// layered dependency chain *looks* like, and it reads fine — you scroll down a
/// diagram far more readily than across one. Treating "far from square" as the
/// fault would throw away the layering on exactly the graphs it suits best.
pub const MAX_WIDTH_RATIO: f64 = 4.0;

/// Below this width nothing is folded, whatever the ratio: a hub over a
/// chain of seven is a wide, shallow drawing and it fits any screen; folding
/// it would only make the chain snake.
pub const MIN_FOLD_WIDTH: i32 = 1440;

/// Does a drawing of this size read, or does it run off the page?
fn fits(w: i32, h: i32) -> bool {
    w <= MIN_FOLD_WIDTH || w as f64 <= h as f64 * MAX_WIDTH_RATIO
}

/// Advance widths for ASCII 32..=126, in thousandths of an em: for each
/// character the wider of Lucida Grande and Arial, measured from the font
/// files rather than assumed.
///
/// A flat average is what made this wrong before. At seven pixels a character
/// "Communications Providers" asks for a 120 box and needs eighteen pixels
/// more than one gives it, because the average is dragged down by letters
/// that name does not contain.
#[rustfmt::skip]
const ADV: [u16; 95] = [
    316, 316, 374, 632, 632, 889, 697, 229, //  !"#$%&'
    333, 333, 482, 795, 316, 579, 316, 524, // ()*+,-./
    632, 632, 632, 632, 632, 632, 632, 632, // 01234567
    632, 632, 316, 316, 795, 795, 795, 556, // 89:;<=>?
    1015, 690, 667, 722, 749, 667, 611, 778, // @ABCDEFG
    735, 288, 500, 667, 556, 861, 739, 778, // HIJKLMNO
    667, 778, 722, 667, 632, 722, 667, 944, // PQRSTUVW
    667, 667, 611, 325, 524, 325, 632, 556, // XYZ[\]^_
    614, 556, 629, 512, 629, 557, 368, 624, // `abcdefg
    621, 289, 304, 584, 289, 934, 621, 614, // hijklmno
    629, 629, 409, 510, 374, 621, 518, 771, // pqrstuvw
    613, 522, 573, 334, 374, 334, 632,      // xyz{|}~
];

/// Archi's default diagram font is 9pt Segoe UI on Windows, 9pt Sans on Linux
/// and Lucida Grande on macOS — twelve pixels of em on all three, since macOS
/// counts points at 72dpi and the others at 96.
const FONT_PX: i32 = 12;

/// What a character outside the table costs. Cyrillic and the Latin
/// supplement sit near here; it is a guess, and a guess that runs wide.
const OTHER_ADV: i32 = 620;

fn advance(c: char) -> i32 {
    match c {
        ' '..='~' => ADV[c as usize - 32] as i32,
        // The punctuation a name is actually written with. An em dash costs
        // three times what the fallback would charge it.
        '—' => 1000,
        '–' | '‑' => 556,
        '’' | '‘' => 229,
        '“' | '”' => 374,
        '…' => 1000,
        '\u{00a0}' => ADV[0] as i32,
        _ => OTHER_ADV,
    }
}

/// How wide a run of text draws, in pixels, rounded up.
fn text_px(s: &str) -> i32 {
    let mils: i32 = s.chars().map(advance).sum();
    (mils * FONT_PX + 999) / 1000
}

/// Word wrap as draw2d does it, returning the number of lines. A word wider
/// than the line gets one to itself and overhangs, which is why the caller
/// sizes the box to the longest word first.
fn wrapped_lines(label: &str, usable: i32) -> i32 {
    let mut lines = 0;
    let mut cur = 0;
    for word in label.split_whitespace() {
        let w = text_px(word);
        if cur == 0 {
            cur = w;
            lines = 1;
        } else if cur + text_px(" ") + w <= usable {
            cur += text_px(" ") + w;
        } else {
            lines += 1;
            cur = w;
        }
    }
    lines.max(1)
}

/// Pixels Archi keeps for itself on each side of a label, inside an element
/// figure whose type icon is showing.
///
/// The figure insets its text control by four pixels; then, when the icon is
/// visible and the text centred — the default for every ArchiMate element —
/// it insets *both* sides by the icon offset instead, so the label stays
/// centred under an icon that is not. That offset runs from 16 to 27 across
/// Archi 5.9's figures, and the widest of them are the motivation elements a
/// model tends to name at length, so this takes the widest rather than the
/// common one: a box a few pixels roomier than it needed to be is not a
/// defect, and a clipped name is.
pub const ICON_INSET: i32 = 27;

/// The same for a figure with no icon — a note, or a group whose label lives
/// in its tab. Only the text control's own margin applies.
pub const TEXT_INSET: i32 = 4;

/// A box sized so its label fits, for an element figure.
///
/// The width is chosen so the name wraps into two lines, from the stock 120
/// up to 264 — a little over two boxes — and the height grows past three
/// lines only when two will not do at that width. A single word longer than
/// the cap wins over the cap: word wrap cannot break inside one, so anything
/// narrower would clip.
pub fn fit_size(label: &str) -> (i32, i32) {
    fit_inside(label, ICON_INSET)
}

/// The same for a figure that shows no type icon, which has far more room.
pub fn fit_note_size(label: &str) -> (i32, i32) {
    fit_inside(label, TEXT_INSET)
}

fn fit_inside(label: &str, inset: i32) -> (i32, i32) {
    const LINE_H: i32 = 15;
    const PAD_H: i32 = 10;
    const MIN_W: i32 = 120;
    const MAX_W: i32 = 264;
    const MIN_H: i32 = 55;

    let chrome = 2 * inset;
    let longest = label.split_whitespace().map(text_px).max().unwrap_or(0);

    // Half the label, so it wraps into two lines, but never narrower than the
    // longest word — and the longest word outranks the cap.
    let want = (text_px(label) / 2).max(longest) + chrome;
    let w = snap_up(want.clamp(MIN_W, MAX_W).max(longest + chrome));

    // Three lines fit the stock height; past that the box grows a line at a
    // time. Snapping up, never to nearest: half a grid square the wrong way
    // is a clipped line.
    let lines = wrapped_lines(label, w - chrome);
    let h = if lines <= 3 { MIN_H } else { snap_up(lines * LINE_H + PAD_H) };
    (w, h)
}

/// Where everything goes, and how the long edges get there.
#[derive(Clone, Debug, Default)]
pub struct Placement {
    /// One rectangle per item, in the order given.
    pub rects: Vec<Rect>,
    /// Which algorithm actually ran. Under [`Algorithm::Auto`] this is what the
    /// fallback chose, so a caller can say so rather than leaving the user to
    /// wonder why the diagram looks like a grid.
    pub algorithm: Algorithm,
}

pub fn place(items: &[Item], edges: &[(usize, usize)], algo: Algorithm) -> Placement {
    match algo {
        Algorithm::Grid => grid_placement(items),
        Algorithm::Sugiyama => sugiyama(items, edges),
        Algorithm::Auto => {
            let layered = sugiyama(items, edges);
            let ratio = wideness(&layered.rects);
            let bbox = layered.rects.iter().copied().reduce(|a, b| a.union(b)).unwrap_or_default();
            if fits(bbox.w, bbox.h) {
                return layered;
            }
            // Folding keeps the layering inside the bound in almost every
            // case; reaching here means the drawing is genuinely wide even
            // folded, which is a rank of very wide boxes.
            //
            // The grid is squarer, but it places by name and ignores every
            // edge, so on a connected graph its lines cross far more: on a set
            // of real views, three to ten times as often. A drawing five times
            // wider than tall with a hundred crossings reads; a square one
            // with a thousand does not. So the fallback has to win on both
            // counts — squarer, and no more tangled — or the layering stands.
            // It still catches what it was made for, the shallow graph with
            // almost no edges to tangle, where the grid is as good and much
            // narrower.
            //
            // Tangles are counted as crossings plus edges drawn through a box,
            // which the grid can never avoid — it does not route — and the
            // layering always does. A grid that crosses no more than the
            // layering but runs forty edges through boxes is not the tidier
            // drawing.
            let squared = grid_placement(items);
            let squarer = wideness(&squared.rects) < ratio;
            let (lc, gc) = (tangles(&layered, edges), tangles(&squared, edges));
            let no_worse = gc <= lc;
            if squarer && no_worse { squared } else { layered }
        }
    }
}

/// How tangled a drawing is: crossings between its lines, plus lines drawn
/// through a box, which is worse than a crossing and counted as two.
fn tangles(p: &Placement, edges: &[(usize, usize)]) -> usize {
    let through = edges
        .iter()
        .filter(|(a, b)| a != b && !straight_is_clear(p.rects[*a], p.rects[*b], &p.rects, *a, *b))
        .count();
    drawn_crossings(p, edges) + 2 * through
}

/// Crossings between the straight centre-to-centre lines of a placement.
///
/// The routed drawing bends some of these, so this is an estimate rather than
/// the count of the finished picture — but it is the same estimate for both
/// placements being compared, which is what the comparison needs.
fn drawn_crossings(p: &Placement, edges: &[(usize, usize)]) -> usize {
    let segs: Vec<(Pt, Pt, usize, usize)> = edges
        .iter()
        .filter(|(a, b)| a != b)
        .map(|&(a, b)| (p.rects[a].center(), p.rects[b].center(), a, b))
        .collect();
    let mut n = 0;
    for (i, &(p1, p2, a, b)) in segs.iter().enumerate() {
        for &(q1, q2, c, d) in &segs[i + 1..] {
            if a == c || a == d || b == c || b == d {
                continue;
            }
            if segments_cross(p1, p2, q1, q2) {
                n += 1;
            }
        }
    }
    n
}

/// Do two open segments properly cross?
fn segments_cross(p1: Pt, p2: Pt, q1: Pt, q2: Pt) -> bool {
    let o = |a: Pt, b: Pt, c: Pt| -> i64 {
        (b.x - a.x) as i64 * (c.y - a.y) as i64 - (b.y - a.y) as i64 * (c.x - a.x) as i64
    };
    let (d1, d2, d3, d4) = (o(p1, p2, q1), o(p1, p2, q2), o(q1, q2, p1), o(q1, q2, p2));
    d1 != 0 && d2 != 0 && d3 != 0 && d4 != 0 && (d1 > 0) != (d2 > 0) && (d3 > 0) != (d4 > 0)
}

fn grid_placement(items: &[Item]) -> Placement {
    Placement { rects: grid(items), algorithm: Algorithm::Grid }
}

/// Width of the bounding box over its height, so 1.0 is square and 10.0 is a
/// letterbox.
fn wideness(rects: &[Rect]) -> f64 {
    let Some(bbox) = rects.iter().copied().reduce(|a, b| a.union(b)) else { return 1.0 };
    bbox.w.max(1) as f64 / bbox.h.max(1) as f64
}

fn snap(v: i32) -> i32 {
    (v as f64 / GRID as f64).round() as i32 * GRID
}

/// Snap outwards. Sizing rounds this way: `snap` can take six pixels off a
/// width that was measured, and a measured width has no six pixels to spare.
fn snap_up(v: i32) -> i32 {
    (v + GRID - 1) / GRID * GRID
}

/// How wide a row runs once its slots are laid end to end.
fn row_width(row: &[Slot], width: &HashMap<Slot, i32>) -> i32 {
    let mut w = row.first().map(|s| width[s]).unwrap_or(0);
    for pair in row.windows(2) {
        w += width[&pair[1]] + gap_between(&pair[0], &pair[1]);
    }
    w
}

/// The room left between two slots side by side. Boxes get the full gap;
/// a corridor beside a box half of it; two corridors none — a corridor is
/// already as wide as its line's sweep plus a lane, so lines in adjacent
/// corridors are a lane apart, and a fan of twenty long edges through a row
/// costs the row twenty lanes rather than twenty gaps.
fn gap_between(a: &Slot, b: &Slot) -> i32 {
    match (matches!(a, Slot::Dummy(..)), matches!(b, Slot::Dummy(..))) {
        (false, false) => HGAP,
        (true, true) => 0,
        _ => HGAP / 2,
    }
}

/// The drawing a set of rows would come out as, before any of it is placed.
///
/// Height is counted exactly as [`Layered::assign_y`] will count it, so the
/// ratio the fold is judged against is the one the drawing actually gets.
fn extent(rows: &[Vec<Slot>], width: &HashMap<Slot, i32>, pitch: i32) -> (i32, i32) {
    let w = rows.iter().map(|r| row_width(r, width)).max().unwrap_or(0);
    let n = rows.len() as i32;
    let h = if n == 0 { 0 } else { n * pitch + (n - 1) * VGAP };
    (w, h)
}

/// What a fold produces: the rows, which folded rank each is a line of, and
/// where each box of a folded rank stood before.
struct Folded {
    rows: Vec<Vec<Slot>>,
    fold_of: Vec<Option<usize>>,
    proj: HashMap<Slot, f64>,
}

/// Break rows wider than `budget` into lines, leaving the rest untouched.
///
/// The lines nest. The first keeps the row's two ends and the last gets its
/// middle, so seen from above the lines still read in the row's order, with
/// the inner lines' boxes standing between the outer lines' — that order was
/// chosen to keep edges from crossing, and it is kept. It is also what
/// keeps the drawing clean: an edge from above to a box on an inner line
/// crosses the outer lines where that box would have stood, which is between
/// the outer boxes and not through them, and the corridors of all the inner
/// boxes lie together in one block that the outer boxes flank. Cut the row
/// into consecutive chunks instead and every inner box's corridor lands
/// among the outer boxes wherever the interpolation puts it; a fan of
/// thirteen was folded into two lines of six and seven with the far six
/// scattered across the near seven, and half their lines went through a
/// box.
///
/// This runs before any corridor exists, so every row is free to fold; the
/// corridors are laid afterwards, by row, and thread through the lines like
/// any other row.
fn fold_rows(
    rows: &[Vec<Slot>],
    budget: i32,
    width: &HashMap<Slot, i32>,
    far_below: &[bool],
) -> Folded {
    let mut out: Vec<Vec<Slot>> = Vec::with_capacity(rows.len());
    let mut fold_of: Vec<Option<usize>> = Vec::with_capacity(rows.len());
    let mut proj: HashMap<Slot, f64> = HashMap::new();
    for (rank, row) in rows.iter().enumerate() {
        if row.len() < 2 || row_width(row, width) <= budget {
            out.push(row.clone());
            fold_of.push(None);
            continue;
        }
        // Take the number of lines first and the share per line from it, so the
        // fold comes out balanced. Filling each line to the budget instead
        // would leave the last one holding a single box.
        let budget = budget.max(1);
        let lines = ((row_width(row, width) + budget - 1) / budget) as usize;
        let per = row.len().div_ceil(lines.max(1)).max(1);
        let n = row.len() as f64;
        for (i, s) in row.iter().enumerate() {
            proj.insert(*s, (i as f64 + 0.5) / n);
        }
        // Peel `per` off the two ends for each outer line; what is left in
        // the middle is the innermost. The outer line goes nearest the rows
        // this rank's edges lead to and the inner lines away from them, so
        // an inner box's edge crosses the outer lines where the box would
        // have stood — between the outer boxes — and never through them.
        let mut lines: Vec<Vec<Slot>> = Vec::new();
        let mut rest: &[Slot] = row;
        while rest.len() > per {
            let left = per.div_ceil(2);
            let right = per - left;
            let mut line = rest[..left].to_vec();
            line.extend_from_slice(&rest[rest.len() - right..]);
            lines.push(line);
            rest = &rest[left..rest.len() - right];
        }
        lines.push(rest.to_vec());
        if !far_below.get(rank).copied().unwrap_or(true) {
            lines.reverse();
        }
        for line in lines {
            out.push(line);
            fold_of.push(Some(rank));
        }
    }
    Folded { rows: out, fold_of, proj }
}

fn grid(items: &[Item]) -> Vec<Rect> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| key(items, *a).cmp(&key(items, *b)));

    let cols = (items.len() as f64).sqrt().ceil().max(1.0) as usize;
    let cell_w = items.iter().map(|i| i.w).max().unwrap_or(120) + HGAP;
    let cell_h = items.iter().map(|i| i.h).max().unwrap_or(55) + VGAP;

    let mut out = vec![Rect::default(); items.len()];
    for (slot, &i) in order.iter().enumerate() {
        out[i] = Rect {
            x: snap((slot % cols) as i32 * cell_w),
            y: snap((slot / cols) as i32 * cell_h),
            w: items[i].w,
            h: items[i].h,
        };
    }
    out
}

fn key(items: &[Item], i: usize) -> (&str, &str) {
    (items[i].name.as_str(), items[i].id.as_str())
}

/// Rank the dependency graph, with cycles broken first.
///
/// Every edge points downward and, before dummies, spans at least one row.
/// Longest path gives a first layering with that property; network simplex
/// then shortens it — see [`network_simplex`] for why the first one is not
/// good enough.
fn rank_by_dependency(n: usize, edges: &[(usize, usize)]) -> (Vec<usize>, HashSet<usize>) {
    // Break cycles by reversing back-edges found in a depth-first walk. Doing
    // this rather than dropping them keeps the relationship on the diagram; it
    // just points the other way for layout purposes.
    let mut reversed: HashSet<usize> = HashSet::new();
    let mut state = vec![0u8; n]; // 0 unvisited, 1 on stack, 2 done
    let mut out_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (ei, (a, b)) in edges.iter().enumerate() {
        if a != b {
            out_edges[*a].push((*b, ei));
        }
    }

    for start in 0..n {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = 1;
        while let Some((v, i)) = stack.pop() {
            if i < out_edges[v].len() {
                stack.push((v, i + 1));
                let (w, ei) = out_edges[v][i];
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => {
                        reversed.insert(ei);
                    }
                    _ => {}
                }
            } else {
                state[v] = 2;
            }
        }
    }

    // Longest path, by relaxing along a topological order.
    let mut indeg = vec![0usize; n];
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ei, (a, b)) in edges.iter().enumerate() {
        if a == b || reversed.contains(&ei) {
            continue;
        }
        succ[*a].push(*b);
        indeg[*b] += 1;
    }
    let mut rank = vec![0usize; n];
    let mut queue: VecDeque<usize> = (0..n).filter(|v| indeg[*v] == 0).collect();
    let mut seen = 0;
    while let Some(v) = queue.pop_front() {
        seen += 1;
        for &w in &succ[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    debug_assert_eq!(seen, n, "cycle breaking should have left a DAG");

    // Longest path is a valid layering, but a poor one: it puts every node as
    // high as it can go, so a node whose only successor is four rows down is
    // dragged to the top and its edge crosses three rows for nothing. Worse,
    // a sink is anchored wherever the longest chain into it happens to end,
    // and on a real model that is the wrong anchor. A hub reached by one
    // eight-step flow chain and by thirty single hops sits eight ranks deep,
    // and all thirty hops are stretched to reach it — thirty long edges, each
    // drawn across every rank between and crossing whatever is there. One
    // view of eighty-six concepts, three hops from its hub in every direction,
    // came out fifteen ranks tall.
    //
    // Network simplex fixes that globally: it moves whole subtrees to
    // minimise the total span of every edge, so the chain is what gives — it
    // is one edge per rank however it is drawn — and the thirty hops become
    // one row each. Longest path is only the starting point it needs.
    // The DAG the simplex works on: every edge pointing down, self-loops out.
    let dag: Vec<(usize, usize)> = edges
        .iter()
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(ei, (a, b))| if reversed.contains(&ei) { (*b, *a) } else { (*a, *b) })
        .collect();
    let rank = network_simplex(n, &dag, rank);

    (rank, reversed)
}

/// Rank to minimise the total length of every edge, keeping each at least one
/// rank long.
///
/// This is the ranking step of Gansner et al.'s "A Technique for Drawing
/// Directed Graphs" (1993), the same one `dot` uses. The problem is a linear
/// programme — minimise Σ(rank[b] − rank[a]) subject to rank[b] − rank[a] ≥ 1
/// for every edge — and network simplex solves it on the graph directly:
///
/// * A spanning tree of *tight* edges (length exactly 1) is a basic feasible
///   solution: fix any node's rank and the tree determines the rest.
/// * Every tree edge splits the tree in two. Its **cut value** is the total
///   length change of moving the tail component one rank closer to the head
///   component: negative means the drawing gets shorter if that edge leaves
///   the tree and the components slide together until some other edge goes
///   tight and takes its place.
/// * Pivot on negative cut values until there are none. That is optimal.
///
/// The starting tree grows from the longest-path ranking, which is feasible by
/// construction; nodes it does not reach — because no edge into or out of them
/// is tight — are pulled in by shifting a component until one is.
///
/// Deterministic throughout: nodes are visited in index order, tree edges are
/// scanned in the order they were added, and the first negative cut value is
/// the one pivoted on. Isolated nodes keep rank zero.
fn network_simplex(n: usize, edges: &[(usize, usize)], init: Vec<usize>) -> Vec<usize> {
    if n == 0 || edges.is_empty() {
        return init;
    }
    let m = edges.len();
    let mut rank: Vec<i64> = init.iter().map(|&r| r as i64).collect();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ei, (a, b)) in edges.iter().enumerate() {
        adj[*a].push(ei);
        adj[*b].push(ei);
    }
    let len = |rank: &[i64], ei: usize| rank[edges[ei].1] - rank[edges[ei].0];

    // ---- feasible tight tree ------------------------------------------------
    // Grow from node 0's component: repeatedly take every tight edge into the
    // tree; when it stalls short of spanning, find the non-tree edge with the
    // least slack that touches the tree, shift the whole tree by that slack so
    // it goes tight, and continue. Nodes in other components are attached the
    // same way when their turn comes.
    let mut in_tree = vec![false; n];
    let mut tree_edges: Vec<usize> = Vec::new();
    let mut is_tree_edge = vec![false; m];
    let mut tree_size = 0;
    let mut start = 0;
    while tree_size < n {
        // Seed a new component with the first node not yet in the tree.
        while start < n && in_tree[start] {
            start += 1;
        }
        if start >= n {
            break;
        }
        in_tree[start] = true;
        tree_size += 1;
        let mut component = vec![start];
        loop {
            // Add every tight edge from the component.
            let mut grew = true;
            while grew {
                grew = false;
                let mut i = 0;
                while i < component.len() {
                    let v = component[i];
                    for &ei in &adj[v] {
                        if is_tree_edge[ei] {
                            continue;
                        }
                        let (a, b) = edges[ei];
                        let w = if a == v { b } else { a };
                        if !in_tree[w] && len(&rank, ei) == 1 {
                            in_tree[w] = true;
                            is_tree_edge[ei] = true;
                            tree_edges.push(ei);
                            component.push(w);
                            tree_size += 1;
                            grew = true;
                        }
                    }
                    i += 1;
                }
            }
            // Component is closed under tight edges. Is there a slack edge from
            // it to a node outside the whole tree? Then shift the component to
            // make the tightest one tight and go round again.
            let mut best: Option<(i64, usize, i64)> = None; // (slack, edge, direction)
            for &v in &component {
                for &ei in &adj[v] {
                    let (a, b) = edges[ei];
                    let w = if a == v { b } else { a };
                    if in_tree[w] {
                        continue;
                    }
                    let l = len(&rank, ei);
                    let slack = l - 1;
                    // The component moves toward w: down if v is the tail
                    // (a == v), up if v is the head.
                    let dir = if a == v { 1 } else { -1 };
                    if best.is_none_or(|(s, e, _)| slack < s || (slack == s && ei < e)) {
                        best = Some((slack, ei, dir));
                    }
                }
            }
            match best {
                Some((slack, _, dir)) => {
                    for &v in &component {
                        rank[v] += dir * slack;
                    }
                }
                None => break, // this component is done; another may remain
            }
        }
    }
    debug_assert_eq!(tree_size, n);

    // ---- pivot ---------------------------------------------------------------
    // Cut values need, for each tree edge, which side each node is on. With
    // the tree rooted at 0 and each node's subtree described by a DFS interval
    // [low, lim], "u is in the subtree under tree edge e" is an interval test.
    // Recomputed after every pivot — simple, and fast enough for a view.
    let mut tree_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &ei in &tree_edges {
        let (a, b) = edges[ei];
        tree_adj[a].push(ei);
        tree_adj[b].push(ei);
    }

    for _iteration in 0..(4 * m + 16) {
        // Root at 0, compute low/lim and each node's parent tree edge.
        let mut low = vec![0usize; n];
        let mut lim = vec![0usize; n];
        let mut parent_edge: Vec<Option<usize>> = vec![None; n];
        let mut counter = 0;
        let mut visited = vec![false; n];
        // Iterative DFS over every component (the tree spans everything now).
        for root in 0..n {
            if visited[root] {
                continue;
            }
            let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
            visited[root] = true;
            low[root] = counter;
            while let Some((v, i)) = stack.pop() {
                if i < tree_adj[v].len() {
                    stack.push((v, i + 1));
                    let ei = tree_adj[v][i];
                    let (a, b) = edges[ei];
                    let w = if a == v { b } else { a };
                    if !visited[w] {
                        visited[w] = true;
                        parent_edge[w] = Some(ei);
                        low[w] = counter;
                        counter += 1;
                        stack.push((w, 0));
                    }
                } else {
                    lim[v] = counter;
                    counter += 1;
                }
            }
        }
        // For each tree edge, the child is whichever endpoint has it as parent
        // edge; the "tail component" is the child's subtree.
        let child_of = |ei: usize| -> usize {
            let (a, b) = edges[ei];
            if parent_edge[a] == Some(ei) { a } else { b }
        };
        let in_subtree = |u: usize, c: usize| low[c] <= low[u] && lim[u] <= lim[c];

        // Cut value of a tree edge: over every graph edge crossing the cut it
        // makes, +1 for one running the same way as the tree edge and −1 for
        // one running the other way. Negative means more edges would shorten
        // than lengthen if the two sides slid together, so the tree edge
        // should leave. The most negative is taken, which is the textbook
        // choice and needs fewer pivots than the first negative found.
        let mut leave: Option<(i64, usize)> = None;
        for &ei in &tree_edges {
            let c = child_of(ei);
            let (ea, _) = edges[ei];
            let e_tail_in_c = in_subtree(ea, c);
            let mut cut: i64 = 0;
            for (fa, fb) in edges {
                let a_in = in_subtree(*fa, c);
                if a_in == in_subtree(*fb, c) {
                    continue;
                }
                cut += if a_in == e_tail_in_c { 1 } else { -1 };
            }
            if cut < 0 && leave.is_none_or(|(c0, _)| cut < c0) {
                leave = Some((cut, ei));
            }
        }
        let Some((_, leave_ei)) = leave else { break };

        // Entering edge: among non-tree edges crossing the same cut in the
        // opposite direction to `leave`, the one with least slack.
        let c = child_of(leave_ei);
        let (la, _) = edges[leave_ei];
        let l_tail_in_c = in_subtree(la, c);
        let mut enter: Option<(i64, usize)> = None;
        for (fi, (fa, fb)) in edges.iter().enumerate() {
            if is_tree_edge[fi] {
                continue;
            }
            let a_in = in_subtree(*fa, c);
            let b_in = in_subtree(*fb, c);
            if a_in == b_in {
                continue;
            }
            let f_same_dir = a_in == l_tail_in_c;
            if f_same_dir {
                continue;
            }
            let slack = len(&rank, fi) - 1;
            if enter.is_none_or(|(s, e)| slack < s || (slack == s && fi < e)) {
                enter = Some((slack, fi));
            }
        }
        let Some((slack, enter_ei)) = enter else {
            // No candidate means the cut is unbounded that way, which cannot
            // happen with the constraint set here; stop rather than loop.
            break;
        };

        // Slide c's subtree by `slack` so the entering edge goes tight, then
        // exchange the edges. The entering edge runs opposite to the leaving
        // one across the cut, so if the leaving edge's tail is on c's side the
        // entering edge's head is, and it shortens when c's side moves up.
        let delta = if l_tail_in_c { -slack } else { slack };
        for (v, r) in rank.iter_mut().enumerate() {
            if in_subtree(v, c) {
                *r += delta;
            }
        }
        is_tree_edge[leave_ei] = false;
        is_tree_edge[enter_ei] = true;
        if let Some(p) = tree_edges.iter().position(|&e| e == leave_ei) {
            tree_edges[p] = enter_ei;
        }
        // Rebuild tree adjacency.
        for l in tree_adj.iter_mut() {
            l.clear();
        }
        for &ei in &tree_edges {
            let (a, b) = edges[ei];
            tree_adj[a].push(ei);
            tree_adj[b].push(ei);
        }
    }

    // Normalise so the smallest rank is zero, and never let anything be
    // negative or unset.
    let min = rank.iter().copied().min().unwrap_or(0);
    rank.iter().map(|&r| (r - min).max(0) as usize).collect()
}

/// Lay out each connected component on its own, then pack the components.
///
/// Laid out together, components interfere: they share ranks, so a fold cuts
/// across them and ten unconnected pairs come out as two folded rows with
/// every pair's edge slanting across the fold; and the placement sweeps,
/// which pull a node toward its neighbours, have nothing to say about where
/// one component sits relative to another, so one drifts off to the right
/// during the sweeps and — with no edge crossing the gap for the compaction
/// to shorten — is never pulled back, leaving two thousand pixels of nothing.
///
/// Apart, each component is a drawing of its own — a pair is two boxes one
/// above the other — and the packing decides where they go: largest first,
/// left to right, a shelf at a time, wrapping when a shelf would run past
/// the width bound. Nothing else can put daylight between them.
fn sugiyama(items: &[Item], edges: &[(usize, usize)]) -> Placement {
    let mut comps = components(items.len(), edges);
    let pitch = items.iter().map(|i| i.h).max().unwrap_or(55).max(55);
    if comps.len() <= 1 {
        return sugiyama_connected(items, edges, pitch);
    }
    // Largest first, then by the name and id of the first member — content,
    // not index, so reversing the input does not reorder the packing.
    let first_key = |c: &Vec<usize>| {
        c.iter().map(|i| key(items, *i)).min().map(|(n, id)| (n.to_string(), id.to_string()))
    };
    comps.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| first_key(a).cmp(&first_key(b))));

    // Each component laid out in its own coordinates, with a mapping back to
    // the caller's indices for both items and edges.
    let mut drawn: Vec<(Vec<usize>, Placement)> = Vec::with_capacity(comps.len());
    for members in &comps {
        let local: HashMap<usize, usize> =
            members.iter().enumerate().map(|(li, gi)| (*gi, li)).collect();
        let sub_items: Vec<Item> = members.iter().map(|gi| items[*gi].clone()).collect();
        let sub_edges: Vec<(usize, usize)> =
            edges.iter().filter_map(|(a, b)| Some((*local.get(a)?, *local.get(b)?))).collect();
        let p = sugiyama_connected(&sub_items, &sub_edges, pitch);
        drawn.push((members.clone(), p));
    }

    // Pack: shelves left to right, wrapping at the width bound. The bound is
    // taken from the total area, the same way the fold judges a row: a
    // drawing of area A drawn r times wider than tall is sqrt(A·r) across.
    let extent = |p: &Placement| -> Rect {
        p.rects.iter().copied().reduce(|a, b| a.union(b)).unwrap_or_default()
    };
    let bbox = |p: &Placement| -> (i32, i32) {
        let b = extent(p);
        (b.w, b.h)
    };
    // The shelf width is what a square-ish packing of the whole set would
    // come to, from its area — the same sqrt(A·r) the fold uses — but never
    // less than the widest component, and never so tight that a shelf holds
    // one component when two would still be inside the bound. Wrapping is
    // decided against that width.
    let area: f64 = drawn
        .iter()
        .map(|(_, p)| {
            let (w, h) = bbox(p);
            (w + HGAP) as f64 * (h + VGAP) as f64
        })
        .sum();
    let widest = drawn.iter().map(|(_, p)| bbox(p).0).max().unwrap_or(0);
    let tallest = drawn.iter().map(|(_, p)| bbox(p).1).max().unwrap_or(0);
    let by_area = (area * MAX_WIDTH_RATIO).sqrt() as i32;
    // A single shelf as wide as the bound allows for the tallest component.
    let by_ratio = (tallest as f64 * MAX_WIDTH_RATIO) as i32;
    let shelf_w = by_area.max(by_ratio).max(widest);

    let mut rects = vec![Rect::default(); items.len()];
    let (mut x, mut y, mut shelf_h) = (0, 0, 0);
    for (members, p) in &drawn {
        let (w, h) = bbox(p);
        if x > 0 && x + w > shelf_w {
            x = 0;
            y = snap(y + shelf_h + VGAP);
            shelf_h = 0;
        }
        // Move by whole grid steps, so a route point that overhangs the
        // boxes by half a lane cannot pull the boxes off the grid.
        let origin = extent(p);
        let (dx, dy) = (snap(x - origin.x), snap(y - origin.y));
        for (li, gi) in members.iter().enumerate() {
            let r = p.rects[li];
            rects[*gi] = Rect { x: r.x + dx, y: r.y + dy, w: r.w, h: r.h };
        }
        x = snap(x + w + HGAP);
        shelf_h = shelf_h.max(h);
    }
    Placement { rects, algorithm: Algorithm::Sugiyama }
}

/// The connected components of the graph, each a sorted list of item
/// indices, in discovery order. The caller sorts them by content, so that the
/// packing is determined by the graph and not by the order it was given in.
fn components(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, b) in edges {
        if a != b {
            adj[*a].push(*b);
            adj[*b].push(*a);
        }
    }
    let mut seen = vec![false; n];
    let mut out: Vec<Vec<usize>> = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut members = Vec::new();
        seen[start] = true;
        while let Some(v) = stack.pop() {
            members.push(v);
            for &w in &adj[v] {
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        members.sort_unstable();
        out.push(members);
    }
    out
}

/// One connected component, laid out. `min_pitch` is the row height the
/// whole drawing uses, so components packed side by side share it.
///
/// Several layerings are tried and the least tangled drawing is kept. The
/// first follows the arrows: every edge points down, ranks by network
/// simplex. The others ignore direction and grow rings out from a hub —
/// see [`radial_layers`] — which is what makes a hub's fan sit on both
/// sides of it, a chain lie along one row, and two hubs sharing a crowd
/// take opposite sides of it, none of which a drawing that must point every
/// arrow down can do. Direction is a reading aid; a line through a box or
/// across another line is a reading obstacle, so the obstacle count decides
/// and the arrows are kept only when they cost nothing. On a tie the drawing
/// with fewer long edges wins, then the one with fewer rows — a chain along
/// a row under its hub rather than snaked over three — then the smaller,
/// then the directed one.
fn sugiyama_connected(items: &[Item], edges: &[(usize, usize)], min_pitch: i32) -> Placement {
    let n = items.len();
    let (raw_rank, _) = rank_by_dependency(n, edges);
    let mut candidates: Vec<Vec<usize>> = vec![compact(&raw_rank)];
    for root in radial_roots(items, edges) {
        let layers = compact(&radial_layers(items, edges, root));
        if !candidates.contains(&layers) {
            candidates.push(layers);
        }
    }

    let mut best: Option<(Placement, (usize, usize, usize, i64))> = None;
    for layers in &candidates {
        let g = build(items, edges, layers, min_pitch);
        let rects = g.finish(items);
        let p = Placement { rects, algorithm: Algorithm::Sugiyama };
        let long = g.rows.iter().flatten().filter(|s| matches!(s, Slot::Dummy(..))).count();
        let bbox = p.rects.iter().copied().reduce(|a, b| a.union(b)).unwrap_or_default();
        let score = (tangles(&p, edges), long, g.rows.len(), bbox.w as i64 * bbox.h as i64);
        if best.as_ref().is_none_or(|(_, s)| score < *s) {
            best = Some((p, score));
        }
    }
    best.map(|(p, _)| p).unwrap_or_default()
}

/// The hubs worth growing rings from: the three of highest degree, ties by
/// name and id so the choice is the graph's and not the input order's.
fn radial_roots(items: &[Item], edges: &[(usize, usize)]) -> Vec<usize> {
    let mut degree = vec![0usize; items.len()];
    for (a, b) in edges {
        if a != b {
            degree[*a] += 1;
            degree[*b] += 1;
        }
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| {
        degree[*b].cmp(&degree[*a]).then_with(|| key(items, *a).cmp(&key(items, *b)))
    });
    order.truncate(3);
    order
}

/// Layers by distance from `root`, on both sides of it.
///
/// Rings of a breadth-first search: the root is layer zero, its neighbours
/// are one ring out, theirs two, and so on. Each node then takes the ring
/// on the side most of its already-placed neighbours are on — the side of
/// its parent, so a subtree stays together — and, when that is a tie, the
/// side that is currently narrower at that ring, so a hub's fan splits in
/// half above and below it rather than running along one row.
///
/// Two nodes in the same ring on the same side share a row, and an edge
/// between them is drawn along the row, between adjacent boxes. That only
/// works while such edges form paths — a box has two sides — so a node
/// whose arrival would give a same-row neighbour a third such edge, or
/// close a cycle, is moved one ring further out instead: its edge to its
/// parent then crosses one row, which the corridors handle.
///
/// The result is direction-blind, which is the point: an arrow read upward
/// costs nothing next to a line drawn through a box.
fn radial_layers(items: &[Item], edges: &[(usize, usize)], root: usize) -> Vec<usize> {
    let n = items.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, b) in edges {
        if a != b {
            adj[*a].push(*b);
            adj[*b].push(*a);
        }
    }
    for (v, list) in adj.iter_mut().enumerate() {
        list.sort_by(|a, b| key(items, *a).cmp(&key(items, *b)));
        list.dedup();
        list.retain(|w| *w != v);
    }

    // Breadth-first distance, visiting neighbours by name so the order is
    // the graph's.
    let mut dist = vec![usize::MAX; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    dist[root] = 0;
    let mut queue: VecDeque<usize> = VecDeque::from([root]);
    while let Some(v) = queue.pop_front() {
        order.push(v);
        for &w in &adj[v] {
            if dist[w] == usize::MAX {
                dist[w] = dist[v] + 1;
                queue.push_back(w);
            }
        }
    }

    // Signed layer per node; the root is zero. `same_deg` counts a node's
    // edges drawn along its own row, and `path_root` finds the path it is on,
    // so a third such edge or a cycle is refused.
    let mut layer: Vec<Option<i64>> = vec![None; n];
    layer[root] = Some(0);
    let mut same_deg = vec![0usize; n];
    let mut path_root: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut v: usize) -> usize {
        while p[v] != v {
            p[v] = p[p[v]];
            v = p[v];
        }
        v
    }
    // Width placed so far on each signed layer, for the balance.
    let mut width: HashMap<i64, i32> = HashMap::new();
    let deepest = order.last().map(|v| dist[*v]).unwrap_or(0);

    for d in 1..=deepest {
        // The ring, and within it the groups that belong together: nodes
        // joined by an edge of their own — a chain reached from a hub — and
        // nodes with a common neighbour further out — the crowd a second hub
        // shares with the first. A group takes one side together, the side
        // most of its parents are on, so a chain is not cut in two by the
        // hub it hangs from and the second hub's crowd is not split across
        // the first, which would send half its fan through the root's row;
        // balance decides only when the parents do not.
        let ring: Vec<usize> = order.iter().copied().filter(|v| dist[*v] == d).collect();
        let mut grouped = vec![false; n];
        for &start in &ring {
            if grouped[start] {
                continue;
            }
            let mut group = vec![start];
            grouped[start] = true;
            let mut i = 0;
            while i < group.len() {
                let v = group[i];
                for &w in &adj[v] {
                    if dist[w] == d && !grouped[w] {
                        grouped[w] = true;
                        group.push(w);
                    }
                    if dist[w] == d + 1 {
                        for &u in &adj[w] {
                            if dist[u] == d && !grouped[u] {
                                grouped[u] = true;
                                group.push(u);
                            }
                        }
                    }
                }
                i += 1;
            }
            let votes: i64 = group
                .iter()
                .flat_map(|v| adj[*v].iter())
                .filter(|w| dist[**w] < d)
                .filter_map(|w| layer[*w])
                .map(|l| l.signum())
                .sum();
            let sd = d as i64;
            let side = if votes != 0 {
                votes.signum()
            } else if width.get(&sd).copied().unwrap_or(0) <= width.get(&-sd).copied().unwrap_or(0)
            {
                1
            } else {
                -1
            };
            for v in group {
                // The nearest ring on that side where the along-the-row edges
                // still form paths.
                let mut ring = sd;
                let chosen = loop {
                    let l = side * ring;
                    let same: Vec<usize> =
                        adj[v].iter().copied().filter(|w| layer[*w] == Some(l)).collect();
                    let mut roots: Vec<usize> =
                        same.iter().map(|w| find(&mut path_root, *w)).collect();
                    roots.sort_unstable();
                    roots.dedup();
                    let ok = same.len() <= 2
                        && same.iter().all(|w| same_deg[*w] < 2)
                        && roots.len() == same.len();
                    if ok {
                        break l;
                    }
                    ring += 1;
                };
                layer[v] = Some(chosen);
                for w in
                    adj[v].iter().copied().filter(|w| layer[*w] == Some(chosen)).collect::<Vec<_>>()
                {
                    same_deg[w] += 1;
                    same_deg[v] += 1;
                    let (a, b) = (find(&mut path_root, v), find(&mut path_root, w));
                    path_root[a] = b;
                }
                *width.entry(chosen).or_insert(0) += items[v].w + HGAP;
            }
        }
    }

    // Unreached nodes cannot happen on a connected component; give any a
    // layer anyway rather than panic.
    let min = layer.iter().flatten().copied().min().unwrap_or(0);
    layer.iter().map(|l| (l.unwrap_or(0) - min) as usize).collect()
}

/// Squeeze out empty rows so a sparse ranking does not leave gaps.
fn compact(ranks: &[usize]) -> Vec<usize> {
    let mut used: Vec<usize> = ranks.to_vec();
    used.sort_unstable();
    used.dedup();
    let map: HashMap<usize, usize> = used.iter().enumerate().map(|(i, r)| (*r, i)).collect();
    ranks.iter().map(|r| map[r]).collect()
}

// ---- ordering and placement ------------------------------------------------

/// A node in the layered graph: either a real item or a placeholder standing in
/// for an edge passing through this row.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
enum Slot {
    Item(usize),
    /// (edge index, which segment of the chain)
    Dummy(usize, usize),
}

struct Layered {
    rows: Vec<Vec<Slot>>,
    /// Per-slot width, so a dummy reserves a thin lane rather than a whole box.
    width: HashMap<Slot, i32>,
    height: HashMap<Slot, i32>,
    /// Adjacency between consecutive rows, used for crossing counting.
    links: Vec<(Slot, Slot)>,
    x: HashMap<Slot, i32>,
    y: Vec<i32>,
    /// For each row, the rank it is a line of if that rank was folded.
    fold_of: Vec<Option<usize>>,
    /// The height every row is given. At least the tallest box here, and
    /// when this is one component of several, the tallest box in any of
    /// them — so rows line up across the packed drawing.
    pitch: i32,
    /// The gap below each row. Each starts at [`VGAP`] and grows if a
    /// straight edge across it would otherwise cut through a box in one of
    /// its two rows — see [`Self::fit_gap`]. One per gap rather than one for
    /// the drawing, so a single shallow edge makes one gap tall, not all.
    gaps: Vec<i32>,
    /// The two boxes each corridor belongs to, upper end first.
    lane_ends: HashMap<usize, (usize, usize)>,
    /// Boxes joined by an edge along their own row: (path, index along it).
    /// The members of a path stay side by side, in order, through every
    /// reordering, so the edge between two of them is a short line between
    /// neighbours and never runs through a third box.
    glue: HashMap<Slot, (usize, usize)>,
    /// Where a box of a folded rank stood in the rank before it was folded,
    /// as a fraction of the rank. A corridor through one line for an edge
    /// to a box on another line of the same rank takes that box's place, so
    /// the lines of a fold read as one row seen from above.
    proj: HashMap<Slot, f64>,
}

fn build(items: &[Item], edges: &[(usize, usize)], layers: &[usize], min_pitch: i32) -> Layered {
    let depth = layers.iter().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<Slot>> = vec![Vec::new(); depth];
    let mut width = HashMap::new();
    let mut height = HashMap::new();

    for (i, item) in items.iter().enumerate() {
        rows[layers[i]].push(Slot::Item(i));
        width.insert(Slot::Item(i), item.w);
        height.insert(Slot::Item(i), item.h);
    }

    // The edges as they will be drawn: upper end first, self-loops out. An
    // edge whose arrow points up is built the other way round so its
    // corridor descends like everyone else's; one along a row stays as it is
    // and gets no corridor at all.
    let drawn: Vec<Option<(usize, usize)>> = edges
        .iter()
        .map(|(a, b)| {
            let (a, b) = if layers[*a] > layers[*b] { (*b, *a) } else { (*a, *b) };
            (a != b).then_some((a, b))
        })
        .collect();

    let mut g = Layered {
        rows,
        width,
        height,
        links: drawn
            .iter()
            .flatten()
            .filter(|(a, b)| layers[*a] != layers[*b])
            .map(|&(a, b)| (Slot::Item(a), Slot::Item(b)))
            .collect(),
        x: HashMap::new(),
        y: Vec::new(),
        fold_of: Vec::new(),
        pitch: min_pitch.max(items.iter().map(|i| i.h).max().unwrap_or(55)).max(55),
        gaps: Vec::new(),
        lane_ends: HashMap::new(),
        glue: HashMap::new(),
        proj: HashMap::new(),
    };
    g.glue_paths(items, edges, layers);

    // Order on the boxes alone, then fold. Folding wants an ordering to cut
    // into lines, and wants to be free to cut any row: corridors do not exist
    // yet, so no row is pinned by one.
    g.order(items);
    g.fold_to_fit();

    // Now the corridors, one dummy per *row* an edge crosses. Rows, not
    // ranks: a folded rank is several rows, and an edge from one of its lines
    // to the rank below crosses the lines beneath it just as it would cross
    // any other row. Built from ranks, that edge had no corridor and was drawn
    // straight through them.
    //
    // The corridors take room the fold could not see, so the drawing can end
    // a little wider than the fold aimed for. One more fold with the
    // corridors in place — then rebuilt, because folding moves the rows they
    // thread — settles it; a second is never needed in practice, and the loop
    // is bounded either way.
    for round in 0..3 {
        g.lay_corridors(&drawn);
        let (w, h) = extent(&g.rows, &g.width, g.pitch);
        if round == 2 || fits(w, h) {
            break;
        }
        g.strip_corridors();
        g.fold_to_fit();
    }

    // Order again with the corridors in, so they thread between the boxes
    // rather than sitting where they were pushed on at the end of each row.
    g.order(items);
    g.straighten_corridors();
    g.assign_x(items, &drawn_edges(&drawn));
    g.assign_y();
    g
}

/// The drawn edges as index pairs, self-loops out.
fn drawn_edges(drawn: &[Option<(usize, usize)>]) -> Vec<(usize, usize)> {
    drawn.iter().flatten().copied().collect()
}

impl Layered {
    /// Find the paths that edges along a row form, and glue their members.
    ///
    /// The layering promises that such edges form paths; if a layering ever
    /// breaks that promise the offending group is simply not glued, and its
    /// edges are drawn through whatever lies between — which the tangle
    /// count then sees, so that layering loses.
    fn glue_paths(&mut self, items: &[Item], edges: &[(usize, usize)], layers: &[usize]) {
        let n = layers.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (a, b) in edges {
            if a != b && layers[*a] == layers[*b] && !adj[*a].contains(b) {
                adj[*a].push(*b);
                adj[*b].push(*a);
            }
        }
        // Walk each path from the end with the smaller name, so which end
        // is "first" is the graph's choice and not the input order's.
        let mut ends: Vec<usize> = (0..n).filter(|v| adj[*v].len() == 1).collect();
        ends.sort_by(|a, b| key(items, *a).cmp(&key(items, *b)));
        let mut seen = vec![false; n];
        let mut path_id = 0;
        for start in ends {
            if seen[start] {
                continue;
            }
            // Walk from one end of a path to the other. A component with a
            // node of degree three, or a cycle, has no end of degree one to
            // start from, or branches on the way; either leaves it unglued.
            let mut path = vec![start];
            seen[start] = true;
            let (mut prev, mut cur) = (start, adj[start][0]);
            let mut simple = true;
            loop {
                if seen[cur] || adj[cur].len() > 2 {
                    simple = false;
                    break;
                }
                seen[cur] = true;
                path.push(cur);
                let next = adj[cur].iter().copied().find(|w| *w != prev);
                match next {
                    Some(w) => {
                        prev = cur;
                        cur = w;
                    }
                    None => break,
                }
            }
            if simple {
                for (i, v) in path.iter().enumerate() {
                    self.glue.insert(Slot::Item(*v), (path_id, i));
                }
                path_id += 1;
            }
        }
    }

    /// The groups a row is reordered as: each glued path's members that lie
    /// in this row and are consecutive along the path, side by side and in
    /// path order — reversed if the row currently has them that way — and
    /// every other slot on its own.
    ///
    /// The row is repaired on the way: members that had drifted apart are
    /// gathered at the first of them.
    fn groups(&self, row: &[Slot]) -> Vec<Vec<Slot>> {
        let mut out: Vec<Vec<Slot>> = Vec::with_capacity(row.len());
        let mut done: HashSet<Slot> = HashSet::new();
        for s in row {
            if done.contains(s) {
                continue;
            }
            let Some(&(path, _)) = self.glue.get(s) else {
                out.push(vec![*s]);
                continue;
            };
            // Every member of this path in this row, by index along the path.
            let mut members: Vec<(usize, Slot)> = row
                .iter()
                .filter_map(|t| match self.glue.get(t) {
                    Some(&(p, i)) if p == path => Some((i, *t)),
                    _ => None,
                })
                .collect();
            members.sort_unstable();
            // The run that contains `s`: consecutive indices around it.
            let at = members.iter().position(|(_, t)| t == s).unwrap();
            let (mut lo, mut hi) = (at, at);
            while lo > 0 && members[lo - 1].0 + 1 == members[lo].0 {
                lo -= 1;
            }
            while hi + 1 < members.len() && members[hi].0 + 1 == members[hi + 1].0 {
                hi += 1;
            }
            let mut run: Vec<Slot> = members[lo..=hi].iter().map(|(_, t)| *t).collect();
            // Keep the direction the row has them in, if it has one.
            if run.len() > 1 {
                let pos = |t: &Slot| row.iter().position(|u| u == t).unwrap();
                if pos(&run[0]) > pos(&run[run.len() - 1]) {
                    run.reverse();
                }
            }
            for t in &run {
                done.insert(*t);
            }
            out.push(run);
        }
        out
    }

    /// Lay a corridor for every edge that crosses a row: one dummy per row
    /// crossed, chained by links, and a direct link for the rest.
    fn lay_corridors(&mut self, drawn: &[Option<(usize, usize)>]) {
        let row_of: HashMap<usize, usize> = self
            .rows
            .iter()
            .enumerate()
            .flat_map(|(r, row)| {
                row.iter().filter_map(move |s| match s {
                    Slot::Item(i) => Some((*i, r)),
                    _ => None,
                })
            })
            .collect();
        self.links.clear();
        for (ei, d) in drawn.iter().enumerate() {
            let Some((a, b)) = *d else { continue };
            let (r0, r1) = (row_of[&a], row_of[&b]);
            if r0 == r1 {
                continue; // handled at routing time
            }
            let (lo, hi, from, to, upper, lower) = if r0 < r1 {
                (r0, r1, Slot::Item(a), Slot::Item(b), a, b)
            } else {
                (r1, r0, Slot::Item(b), Slot::Item(a), b, a)
            };
            if hi > lo + 1 {
                self.lane_ends.insert(ei, (upper, lower));
            }
            let mut prev = from;
            for (seg, row) in ((lo + 1)..hi).enumerate() {
                let dm = Slot::Dummy(ei, seg);
                self.rows[row].push(dm);
                self.width.insert(dm, DUMMY_W);
                self.height.insert(dm, 0);
                self.links.push((prev, dm));
                prev = dm;
            }
            self.links.push((prev, to));
        }
    }

    /// Take every corridor out again, leaving the boxes where they are.
    fn strip_corridors(&mut self) {
        for row in &mut self.rows {
            row.retain(|s| matches!(s, Slot::Item(_)));
        }
        self.width.retain(|s, _| matches!(s, Slot::Item(_)));
        self.height.retain(|s, _| matches!(s, Slot::Item(_)));
        self.links.clear();
        self.lane_ends.clear();
    }

    /// Which slots each slot is linked to, so a pass asks once per slot rather
    /// than scanning every link for every slot in every row.
    fn neighbours(&self) -> HashMap<Slot, Vec<Slot>> {
        let mut n: HashMap<Slot, Vec<Slot>> = HashMap::new();
        for (a, b) in &self.links {
            n.entry(*a).or_default().push(*b);
            n.entry(*b).or_default().push(*a);
        }
        n
    }

    /// Order within each row to reduce crossings between adjacent rows.
    ///
    /// Two passes alternate, and each is checked against the count of crossings
    /// so that only an improvement is kept.
    ///
    /// The **median** pass puts each slot at the median position of what it
    /// links to in the row it is being aligned against, which is where most of
    /// the reduction comes from. It is a heuristic and stalls: once every slot
    /// sits at its median it stops moving even when swapping two neighbours
    /// would still uncross a pair of edges. That is what the **transpose** pass
    /// is for — it walks each row swapping adjacent slots wherever the swap
    /// removes crossings, and runs to a fixed point. Sweeps continue while
    /// they help and stop at the first that does not.
    ///
    /// Every pass moves *groups*, not slots — see [`Self::groups`] — so the
    /// members of a path drawn along its row stay side by side and in order.
    ///
    /// Everything here is decided by counts of crossings and by `(name, id)`
    /// on a tie, so the result cannot depend on how many sweeps happen to run
    /// or on the order links were pushed in.
    fn order(&mut self, items: &[Item]) {
        for r in 0..self.rows.len() {
            let mut gs = self.groups(&self.rows[r]);
            gs.sort_by_cached_key(|g| g.iter().map(|s| slot_key(items, *s)).min());
            self.rows[r] = gs.concat();
        }
        let nb = self.neighbours();
        let mut best = self.rows.clone();
        let mut best_score = self.crossings();
        if best_score == 0 {
            return;
        }

        // Enough sweeps for the largest views to settle; the early exit below
        // is what usually ends it.
        let mut stale = 0;
        for sweep in 0..24 {
            self.median_pass(items, &nb, sweep % 2 == 0);
            self.transpose_pass(&nb);
            let score = self.crossings();
            // Strictly better only, so the earliest sweep wins a tie and the
            // result cannot depend on how many sweeps happen to run.
            if score < best_score {
                best_score = score;
                best = self.rows.clone();
                stale = 0;
                if score == 0 {
                    break;
                }
            } else {
                stale += 1;
                // Two full down-and-up rounds without a gain: the medians
                // have settled and there is nothing left to transpose.
                if stale >= 4 {
                    break;
                }
            }
        }
        self.rows = best;

        // The median stalls in a local minimum on a dense pair of rows — thirty
        // requirements over thirty drivers with seventy-five edges between them
        // settled at three hundred crossings, and no number of further sweeps
        // moved it. Sifting is what gets it out: each slot in turn is tried at
        // every position in its row and left at the best. It is quadratic in
        // the row and so runs once, on the settled ordering, as a polish.
        if best_score > 0 {
            self.sift(&nb);
        }
    }

    /// Crossings two slots of one row contribute, `u` left of `v`, counted
    /// against the rows above and below. `pos` is every slot's index in its
    /// row; only the neighbouring rows' entries are read.
    fn pair_crossings(
        &self,
        nb: &HashMap<Slot, Vec<Slot>>,
        row_of: &HashMap<Slot, usize>,
        pos: &HashMap<Slot, usize>,
        u: Slot,
        v: Slot,
    ) -> usize {
        let depth = self.rows.len();
        let r = row_of[&u];
        let mut c = 0;
        for other_row in [r.wrapping_sub(1), r + 1] {
            if other_row >= depth {
                continue;
            }
            let us: Vec<usize> = nb
                .get(&u)
                .into_iter()
                .flatten()
                .filter(|o| row_of[o] == other_row)
                .map(|o| pos[o])
                .collect();
            let vs: Vec<usize> = nb
                .get(&v)
                .into_iter()
                .flatten()
                .filter(|o| row_of[o] == other_row)
                .map(|o| pos[o])
                .collect();
            // With u left of v, a crossing is any u-neighbour to the right
            // of a v-neighbour.
            for pu in &us {
                for pv in &vs {
                    if pu > pv {
                        c += 1;
                    }
                }
            }
        }
        c
    }

    /// Crossings between two groups of one row, the first left of the second.
    fn group_crossings(
        &self,
        nb: &HashMap<Slot, Vec<Slot>>,
        row_of: &HashMap<Slot, usize>,
        pos: &HashMap<Slot, usize>,
        left: &[Slot],
        right: &[Slot],
    ) -> usize {
        left.iter()
            .map(|u| {
                right.iter().map(|v| self.pair_crossings(nb, row_of, pos, *u, *v)).sum::<usize>()
            })
            .sum()
    }

    /// Crossings a group's own members make with each other in this order.
    fn inner_crossings(
        &self,
        nb: &HashMap<Slot, Vec<Slot>>,
        row_of: &HashMap<Slot, usize>,
        pos: &HashMap<Slot, usize>,
        group: &[Slot],
    ) -> usize {
        let mut c = 0;
        for i in 0..group.len() {
            for j in i + 1..group.len() {
                c += self.pair_crossings(nb, row_of, pos, group[i], group[j]);
            }
        }
        c
    }

    /// Try every group at every position in its row, keeping the best.
    ///
    /// A group's crossings are counted against the rows above and below only,
    /// which is exact for a move within one row. Repeated while it helps, to a
    /// fixed point.
    fn sift(&mut self, nb: &HashMap<Slot, Vec<Slot>>) {
        let depth = self.rows.len();
        let row_of: HashMap<Slot, usize> = self.row_index();

        for _round in 0..4 {
            let mut improved = false;
            for r in 0..depth {
                let mut groups = self.groups(&self.rows[r]);
                if groups.len() < 3 {
                    continue;
                }
                // Positions in the neighbouring rows, fixed for this row.
                let pos: HashMap<Slot, usize> = [r.wrapping_sub(1), r + 1]
                    .into_iter()
                    .filter(|&o| o < depth)
                    .flat_map(|o| self.rows[o].iter().enumerate().map(move |(i, s)| (*s, i)))
                    .collect();
                // Only groups that actually link to a neighbouring row can
                // change the count.
                let touched: Vec<Vec<Slot>> = groups
                    .iter()
                    .filter(|g| {
                        g.iter().any(|s| {
                            nb.get(s).is_some_and(|ns| ns.iter().any(|o| pos.contains_key(o)))
                        })
                    })
                    .cloned()
                    .collect();
                if touched.len() < 2 {
                    continue;
                }

                for g in touched {
                    // Where it is *now* — earlier moves in this pass have
                    // shifted the indices.
                    let orig_i = groups.iter().position(|t| *t == g).unwrap();
                    // Crossings this group contributes at a position, in the
                    // orientation given.
                    let score = |groups: &[Vec<Slot>], at: usize, g: &[Slot]| -> usize {
                        let mut c = self.inner_crossings(nb, &row_of, &pos, g);
                        for (j, t) in groups.iter().enumerate() {
                            if j == at {
                                continue;
                            }
                            c += if j < at {
                                self.group_crossings(nb, &row_of, &pos, t, g)
                            } else {
                                self.group_crossings(nb, &row_of, &pos, g, t)
                            };
                        }
                        c
                    };
                    let mut best_at = orig_i;
                    let mut best_g = g.clone();
                    let mut best_c = score(&groups, orig_i, &g);
                    // Try every other position, and both ways round. Strictly
                    // better only, and the earliest such position wins, so
                    // the result is fixed.
                    let without: Vec<Vec<Slot>> =
                        groups.iter().filter(|t| **t != g).cloned().collect();
                    let mut flipped = g.clone();
                    flipped.reverse();
                    for at in 0..=without.len() {
                        for cand in [&g, &flipped] {
                            if at == orig_i && *cand == g {
                                continue;
                            }
                            let mut trial = without.clone();
                            trial.insert(at, cand.clone());
                            let c = score(&trial, at, cand);
                            if c < best_c {
                                best_c = c;
                                best_at = at;
                                best_g = cand.clone();
                            }
                        }
                    }
                    if best_at != orig_i || best_g != g {
                        let mut next = without;
                        next.insert(best_at, best_g);
                        groups = next;
                        improved = true;
                    }
                }
                self.rows[r] = groups.concat();
            }
            if !improved {
                break;
            }
        }
    }

    /// Where a slot stands across its row, from 0 to 1.
    ///
    /// For a box on a line of a folded rank it is where the box stood in the
    /// rank before the fold, so the lines of one fold share a scale; for any
    /// other box it is its position among the boxes of its row, corridors
    /// not counted, so a row's scale does not stretch with the corridors
    /// laid through it.
    fn frac(&self, s: Slot) -> f64 {
        if let Some(f) = self.proj.get(&s) {
            return *f;
        }
        let row = self.rows.iter().find(|r| r.contains(&s)).map(Vec::as_slice).unwrap_or(&[]);
        let boxes: Vec<&Slot> = row.iter().filter(|t| matches!(t, Slot::Item(_))).collect();
        let n = boxes.len().max(1) as f64;
        let i = boxes.iter().position(|t| **t == s).unwrap_or(0) as f64;
        (i + 0.5) / n
    }

    /// Move each corridor to where its edge's straight line will run, in the
    /// row's sequence, when that costs no crossings.
    ///
    /// The ordering minimises crossings and nothing else, and a corridor's
    /// place in its row is a matter of indifference to it whenever the count
    /// ties — which it does whenever the corridor's own edge is the only one
    /// touching it, as on a star. Sorted by name it then lands at the end of
    /// the row, and the edge, which is a straight line, slants right across
    /// the row's boxes to reach it. Here each corridor is put where the line
    /// between its ends crosses the row. Through a line of the fold its own
    /// end belongs to, that is where the end stood before the fold — the
    /// lines of a fold read as one row, and the corridor takes its box's
    /// place in it. Anywhere else it is the position that interpolates the
    /// two ends' positions in their rows, so that a vertical edge gets a
    /// vertical corridor and the boxes part around it. The move is kept only
    /// if the crossing count does not rise; the ordering's objective still
    /// comes first.
    fn straighten_corridors(&mut self) {
        let before = self.crossings();
        let saved = self.rows.clone();
        let row_of = self.row_index();
        let mut wanted: HashMap<Slot, f64> = HashMap::new();
        for (ei, (upper, lower)) in &self.lane_ends {
            let (u, l) = (Slot::Item(*upper), Slot::Item(*lower));
            let (Some(&ru), Some(&rl)) = (row_of.get(&u), row_of.get(&l)) else { continue };
            let (fu, fl) = (self.frac(u), self.frac(l));
            let span = (rl - ru) as f64;
            for seg in 0..(rl - ru - 1) {
                let dm = Slot::Dummy(*ei, seg);
                let Some(&r) = row_of.get(&dm) else { continue };
                let fold = self.fold_of.get(r).copied().flatten();
                let f = match fold {
                    Some(k) if self.fold_of.get(rl).copied().flatten() == Some(k) => fl,
                    Some(k) if self.fold_of.get(ru).copied().flatten() == Some(k) => fu,
                    _ => {
                        let t = (seg as f64 + 1.0) / span;
                        fu + (fl - fu) * t
                    }
                };
                wanted.insert(dm, f);
            }
        }
        if wanted.is_empty() {
            return;
        }
        for r in 0..self.rows.len() {
            let groups = self.groups(&self.rows[r]);
            let key = |g: &[Slot]| -> f64 {
                let sum: f64 =
                    g.iter().map(|s| wanted.get(s).copied().unwrap_or_else(|| self.frac(*s))).sum();
                sum / g.len().max(1) as f64
            };
            let mut keyed: Vec<(f64, usize, Vec<Slot>)> =
                groups.into_iter().enumerate().map(|(i, g)| (key(&g), i, g)).collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
            self.rows[r] = keyed.into_iter().flat_map(|(_, _, g)| g).collect();
        }
        // The move can add crossings — a fan of long edges from one hub to
        // boxes spread across a lower rank cannot all be straight and
        // uncrossed at once — so a few transpose passes follow, to take back
        // what they can with the corridors now roughly in place. If it is
        // still worse than before by more than the corridors are worth, it is
        // undone; but a corridor off its line is a line through a box, which
        // counts double, so a corridor in place is worth two crossings.
        let nb = self.neighbours();
        for _ in 0..4 {
            self.transpose_pass(&nb);
        }
        let after = self.crossings();
        if after > before + 2 * wanted.len() {
            self.rows = saved;
        }
    }

    /// One median pass, down or up the rows.
    ///
    /// Each group takes the mean of its members' medians in the row it is
    /// aligned against and the groups with one are sorted by it into the
    /// places left by those without; a group with nothing to align with
    /// holds its place, as `dot` has it, rather than being sorted on its own
    /// index against the others' medians, which are positions in a different
    /// row and not comparable. A group of several is turned round when its
    /// two ends' medians say so.
    fn median_pass(&mut self, items: &[Item], nb: &HashMap<Slot, Vec<Slot>>, down: bool) {
        let sequence: Vec<usize> = if down {
            (1..self.rows.len()).collect()
        } else {
            (0..self.rows.len().saturating_sub(1)).rev().collect()
        };

        for r in sequence {
            let neighbour_row = if down { r - 1 } else { r + 1 };
            let pos: HashMap<Slot, usize> =
                self.rows[neighbour_row].iter().enumerate().map(|(i, s)| (*s, i)).collect();
            let median = |s: &Slot| -> Option<f64> {
                let mut ps: Vec<usize> =
                    nb.get(s).into_iter().flatten().filter_map(|o| pos.get(o).copied()).collect();
                ps.sort_unstable();
                if ps.is_empty() {
                    None
                } else if ps.len() % 2 == 1 {
                    Some(ps[ps.len() / 2] as f64)
                } else {
                    Some((ps[ps.len() / 2 - 1] + ps[ps.len() / 2]) as f64 / 2.0)
                }
            };

            let groups = self.groups(&self.rows[r]);
            let mut movable: Vec<(f64, usize, Vec<Slot>)> = Vec::new();
            let mut fixed: Vec<Option<Vec<Slot>>> = vec![None; groups.len()];
            for (i, g) in groups.into_iter().enumerate() {
                let ms: Vec<f64> = g.iter().filter_map(median).collect();
                if ms.is_empty() {
                    fixed[i] = Some(g);
                    continue;
                }
                let value = ms.iter().sum::<f64>() / ms.len() as f64;
                let mut g = g;
                if g.len() > 1
                    && let (Some(a), Some(b)) = (median(&g[0]), median(&g[g.len() - 1]))
                    && b < a
                {
                    g.reverse();
                }
                movable.push((value, i, g));
            }
            // Ties keep their current order first, so an unconnected slot that
            // was held in place stays there relative to its neighbours, and
            // fall back to the name only when two slots are genuinely
            // interchangeable.
            movable.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| slot_key(items, a.2[0]).cmp(&slot_key(items, b.2[0])))
            });
            let mut moving = movable.into_iter().map(|(_, _, g)| g);
            let mut row: Vec<Slot> = Vec::with_capacity(self.rows[r].len());
            for slot in fixed {
                match slot {
                    Some(g) => row.extend(g),
                    None => row.extend(moving.next().unwrap_or_default()),
                }
            }
            row.extend(moving.flatten());
            self.rows[r] = row;
        }
    }

    /// Swap adjacent groups wherever that removes crossings, and turn a group
    /// round wherever that does, until neither does.
    ///
    /// Each swap is judged locally: only the crossings between the two groups
    /// being swapped and the rows above and below can change, so those are
    /// what is counted, and the whole-drawing count is not recomputed per swap.
    fn transpose_pass(&mut self, nb: &HashMap<Slot, Vec<Slot>>) {
        let depth = self.rows.len();
        // Position of every slot in its row, refreshed as rows change.
        let mut pos: HashMap<Slot, usize> = HashMap::new();
        for row in &self.rows {
            for (i, s) in row.iter().enumerate() {
                pos.insert(*s, i);
            }
        }
        let row_of: HashMap<Slot, usize> = self.row_index();

        let mut improved = true;
        let mut rounds = 0;
        while improved && rounds < 16 {
            improved = false;
            rounds += 1;
            for r in 0..depth {
                let mut groups = self.groups(&self.rows[r]);
                let mut changed = false;
                for g in groups.iter_mut() {
                    if g.len() < 2 {
                        continue;
                    }
                    let mut flipped = g.clone();
                    flipped.reverse();
                    if self.inner_crossings(nb, &row_of, &pos, &flipped)
                        < self.inner_crossings(nb, &row_of, &pos, g)
                    {
                        *g = flipped;
                        changed = true;
                    }
                }
                let mut i = 0;
                while i + 1 < groups.len() {
                    let before =
                        self.group_crossings(nb, &row_of, &pos, &groups[i], &groups[i + 1]);
                    let after = self.group_crossings(nb, &row_of, &pos, &groups[i + 1], &groups[i]);
                    // Strictly fewer, never equal: an equal swap would flip
                    // back on the next round and the pass would never settle.
                    if after < before {
                        groups.swap(i, i + 1);
                        changed = true;
                    }
                    i += 1;
                }
                if changed {
                    self.rows[r] = groups.concat();
                    for (i, s) in self.rows[r].iter().enumerate() {
                        pos.insert(*s, i);
                    }
                    improved = true;
                }
            }
        }
    }

    /// Crossings between each pair of adjacent rows.
    ///
    /// Two edges cross when their endpoints are ordered one way in the upper
    /// row and the other way in the lower — which only makes sense *within* a
    /// pair of rows, so the count is taken pair by pair.
    fn crossings(&self) -> usize {
        let mut total = 0;
        for r in 1..self.rows.len() {
            let upper: HashMap<Slot, usize> =
                self.rows[r - 1].iter().enumerate().map(|(i, s)| (*s, i)).collect();
            let lower: HashMap<Slot, usize> =
                self.rows[r].iter().enumerate().map(|(i, s)| (*s, i)).collect();

            let mut pairs: Vec<(usize, usize)> = self
                .links
                .iter()
                .filter_map(|(a, b)| match (upper.get(a), lower.get(b)) {
                    (Some(u), Some(l)) => Some((*u, *l)),
                    _ => match (upper.get(b), lower.get(a)) {
                        (Some(u), Some(l)) => Some((*u, *l)),
                        _ => None,
                    },
                })
                .collect();

            // Sorted by upper position, a crossing is an inversion in the lower
            // positions, which a merge count finds in n log n rather than n².
            // The largest views have a few hundred links per row pair and this
            // runs once per sweep, so the quadratic loop was where the time
            // went.
            pairs.sort_unstable();
            let lowers: Vec<usize> = pairs.into_iter().map(|(_, l)| l).collect();
            total += inversions(&lowers);
        }
        total
    }

    /// Fold over-wide rows until the drawing fits [`MAX_WIDTH_RATIO`].
    ///
    /// A rank of a hundred siblings is not wrong, it is just unreadable:
    /// nothing stacks, so the row runs off the side of any screen or page. The
    /// fold keeps the ranking — every line of a folded row still sits above the
    /// rank below it — where falling back to a grid throws the ranking away and
    /// sorts the whole diagram by name instead.
    ///
    /// The bound is on the drawing rather than on any one row, and a fold moves
    /// both terms at once: it narrows the drawing and makes it taller. So the
    /// two are searched together, fewest lines first, and the first fold that
    /// fits wins. That is what leaves an ordinary diagram alone — five boxes
    /// over two ranks are already well inside the bound, so the search stops
    /// before folding anything.
    ///
    /// If no fold fits — a single row wider than the bound allows even one
    /// box per line, which does not happen — the rows are left as they were.
    fn fold_to_fit(&mut self) {
        let (w, h) = extent(&self.rows, &self.width, self.pitch);
        if fits(w, h) {
            return;
        }
        let widest = self.rows.iter().map(|r| row_width(r, &self.width)).max().unwrap_or(0);
        let most = self.rows.iter().map(Vec::len).max().unwrap_or(1);
        // Which way each row's edges mostly lead: a row whose edges go up
        // is a crowd under a hub, and its far lines go below.
        let row_of = self.row_index();
        let mut up = vec![0usize; self.rows.len()];
        let mut down = vec![0usize; self.rows.len()];
        for (a, b) in &self.links {
            let (Some(&ra), Some(&rb)) = (row_of.get(a), row_of.get(b)) else { continue };
            if ra < rb {
                down[ra] += 1;
                up[rb] += 1;
            } else if rb < ra {
                down[rb] += 1;
                up[ra] += 1;
            }
        }
        let far_below: Vec<bool> = up.iter().zip(&down).map(|(u, d)| u >= d).collect();
        for lines in 2..=most {
            let budget = (widest + lines as i32 - 1) / lines as i32;
            let folded = fold_rows(&self.rows, budget, &self.width, &far_below);
            let (w, h) = extent(&folded.rows, &self.width, self.pitch);
            if fits(w, h) {
                self.rows = folded.rows;
                self.fold_of = folded.fold_of;
                self.proj = folded.proj;
                return;
            }
        }
    }

    /// Give every slot an x, by Brandes and Köpf.
    ///
    /// "Fast and Simple Horizontal Coordinate Assignment" (2001), the method
    /// `dot`'s successors use. Four layouts are made — aligning each slot
    /// with a median neighbour in the row above or the row below, packing
    /// leftward or rightward — and each slot takes the average of the middle
    /// two of its four positions. Aligned slots share one x exactly, so an
    /// edge between them is vertical, not nearly so; a corridor is a chain of
    /// aligned slots and is a straight column by construction; and packing
    /// is as tight as the boxes allow, so nothing drifts and no group is left
    /// standing off on its own. The priority sweep this replaces got close to
    /// each of those and none of them exactly, and needed a compaction pass
    /// and a fold-block rule to stay out of trouble.
    ///
    /// Then the row gap and the corridor widths, which depend on where the
    /// boxes landed and change how they pack, once round each.
    fn assign_x(&mut self, items: &[Item], edges: &[(usize, usize)]) {
        let _ = items;
        self.brandes_koepf();
        // Corridors sized to the slant their line makes at this placement,
        // then placed again with that room; the gap last, against the boxes
        // as they will be drawn.
        for _ in 0..2 {
            self.fit_gap(edges);
            self.size_lanes();
            self.brandes_koepf();
        }
        self.fit_gap(edges);

        // Shift so the leftmost edge of the drawing sits at zero.
        let min = self.x.values().copied().min().unwrap_or(0);
        for v in self.x.values_mut() {
            *v = snap(*v - min);
        }
    }

    /// The four Brandes–Köpf layouts, balanced.
    fn brandes_koepf(&mut self) {
        let nb = self.neighbours();
        let conflicts = self.type1_conflicts(&nb);
        let mut layouts: Vec<HashMap<Slot, i32>> = Vec::with_capacity(4);
        for (down, left) in [(true, true), (true, false), (false, true), (false, false)] {
            let (root, align) = self.vertical_alignment(&nb, &conflicts, down, left);
            layouts.push(self.horizontal_compaction(&root, &align, left));
        }
        self.balance(layouts);
    }

    /// Non-inner segments that cross an inner one. An inner segment joins two
    /// corridor slots — one long edge passing through two rows — and is kept
    /// straight at any cost; a box's edge that would cross it is not allowed
    /// to align, and is marked so the alignment pass skips it.
    fn type1_conflicts(&self, nb: &HashMap<Slot, Vec<Slot>>) -> HashSet<(Slot, Slot)> {
        let mut marked = HashSet::new();
        let pos = self.positions();
        let is_dummy = |s: &Slot| matches!(s, Slot::Dummy(..));
        for i in 0..self.rows.len().saturating_sub(1) {
            let (upper, lower) = (&self.rows[i], &self.rows[i + 1]);
            let mut k0 = 0usize;
            let mut l = 0usize;
            for l1 in 0..lower.len() {
                let v = lower[l1];
                // Is v the lower end of an inner segment?
                let inner_upper = if is_dummy(&v) {
                    nb.get(&v)
                        .into_iter()
                        .flatten()
                        .find(|u| is_dummy(u) && pos.get(u).map(|(r, _)| *r) == Some(i))
                        .copied()
                } else {
                    None
                };
                if l1 + 1 == lower.len() || inner_upper.is_some() {
                    let k1 = match inner_upper {
                        Some(u) => pos[&u].1,
                        None => upper.len().saturating_sub(1),
                    };
                    while l <= l1 {
                        let w = lower[l];
                        for u in nb.get(&w).into_iter().flatten() {
                            let Some(&(ru, k)) = pos.get(u) else { continue };
                            if ru != i {
                                continue;
                            }
                            if k < k0 || k > k1 {
                                marked.insert((*u, w));
                            }
                        }
                        l += 1;
                    }
                    k0 = k1;
                }
            }
        }
        marked
    }

    /// Row and index of every slot.
    fn positions(&self) -> HashMap<Slot, (usize, usize)> {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.iter().enumerate().map(move |(i, s)| (*s, (r, i))))
            .collect()
    }

    /// Group slots into blocks that will share one x.
    ///
    /// Rows are taken top to bottom (or bottom to top), slots left to right
    /// (or right to left), and each slot is aligned with the median of its
    /// neighbours in the row already done — a corridor slot with a corridor
    /// neighbour there first, so a long edge's chain is one column — provided
    /// that neighbour is not already aligned with someone, the alignment
    /// would not cross one made earlier in this row, and the segment is not
    /// a marked conflict.
    fn vertical_alignment(
        &self,
        nb: &HashMap<Slot, Vec<Slot>>,
        conflicts: &HashSet<(Slot, Slot)>,
        down: bool,
        left: bool,
    ) -> (HashMap<Slot, Slot>, HashMap<Slot, Slot>) {
        let pos = self.positions();
        let mut root: HashMap<Slot, Slot> = HashMap::new();
        let mut align: HashMap<Slot, Slot> = HashMap::new();
        for row in &self.rows {
            for s in row {
                root.insert(*s, *s);
                align.insert(*s, *s);
            }
        }
        let depth = self.rows.len();
        let order: Vec<usize> =
            if down { (0..depth).collect() } else { (0..depth).rev().collect() };
        for (k, &r) in order.iter().enumerate() {
            if k == 0 {
                continue;
            }
            let prev = order[k - 1];
            // Corridor slots claim their neighbours first, boxes after: a
            // long edge's chain, and the box at its end, get their column
            // before a box beside them takes it for a short edge. Brandes
            // and Köpf process a row strictly in order with a single moving
            // guard; taking corridors out of turn needs the guard replaced by
            // the thing it stood for — no two alignments in one row may
            // cross — checked against the alignments made so far.
            let mut slots: Vec<Slot> = if left {
                self.rows[r].clone()
            } else {
                self.rows[r].iter().rev().copied().collect()
            };
            // What each slot would align with, in order of preference: a
            // corridor neighbour first, before any median. For a corridor
            // slot that is what makes the chain a column; for a box it is
            // what puts the end of a long edge on the column, so the whole
            // edge — a straight line — runs down it. Brandes and Köpf align
            // by median only, because their edges bend at the corridor and
            // only the chain need be straight; here the ends must sit on it
            // too. A box with long edges in two columns can sit on one; the
            // median decides which is tried first.
            let candidates = |v: &Slot| -> Vec<(usize, Slot)> {
                let mut ups: Vec<(usize, Slot)> = nb
                    .get(v)
                    .into_iter()
                    .flatten()
                    .filter_map(|u| pos.get(u).filter(|(ru, _)| *ru == prev).map(|(_, i)| (*i, *u)))
                    .collect();
                if ups.is_empty() {
                    return Vec::new();
                }
                ups.sort_unstable();
                ups.dedup();
                let d = ups.len();
                let mut out: Vec<(usize, Slot)> = Vec::new();
                let (m1, m2) = ((d - 1) / 2, d / 2);
                let medians = if left { [m1, m2] } else { [m2, m1] };
                // Nearest the median first, so a hub with long edges in
                // several columns stands over the middle one.
                let mut lanes: Vec<(usize, Slot)> =
                    ups.iter().copied().filter(|(_, u)| matches!(u, Slot::Dummy(..))).collect();
                let mid = ups[medians[0]].0 as i64;
                lanes.sort_by_key(|(i, _)| {
                    ((*i as i64 - mid).abs(), if left { *i as i64 } else { -(*i as i64) })
                });
                out.extend(lanes);
                for m in medians {
                    out.push(ups[m]);
                }
                out
            };
            // Slots whose first choice is the same neighbour — the fan a hub
            // throws into this row, boxes and corridors alike — are taken
            // middle one first, so the hub stands over the middle of its fan.
            // Taken in row order, the fan's first slot would claim the hub
            // and the rest would hang off one side of it. A corridor
            // continuing a corridor still goes before everything: that is
            // the inner segment of a long edge, and it is a column at any
            // cost. Other corridors go before boxes.
            let mut first_choice: HashMap<Slot, Vec<Slot>> = HashMap::new();
            for v in &slots {
                if let Some((_, u)) = candidates(v).first() {
                    first_choice.entry(*u).or_default().push(*v);
                }
            }
            let mut middles: Vec<Slot> = first_choice
                .into_iter()
                .filter(|(_, fan)| fan.len() > 1)
                .map(|(_, fan)| fan[(fan.len() - 1) / 2])
                .collect();
            middles.sort_by_key(|s| pos[s].1);
            let rank = |s: &Slot| -> (u8, usize) {
                let dummy = matches!(s, Slot::Dummy(..));
                let inner = dummy
                    && candidates(s).first().is_some_and(|(_, u)| matches!(u, Slot::Dummy(..)));
                let class = if inner {
                    0
                } else if middles.contains(s) {
                    1
                } else if dummy {
                    2
                } else {
                    3
                };
                (class, pos[s].1)
            };
            slots.sort_by_key(|s| {
                (rank(s).0, if left { rank(s).1 } else { usize::MAX - rank(s).1 })
            });
            // Alignments made in this row, as (upper index, lower index).
            let mut made: Vec<(usize, usize)> = Vec::new();
            for v in slots {
                let vj = pos[&v].1;
                let candidates = candidates(&v);
                if candidates.is_empty() {
                    continue;
                }
                for (i, u) in candidates {
                    if align[&v] != v {
                        break;
                    }
                    if conflicts.contains(&(u, v)) || conflicts.contains(&(v, u)) {
                        continue;
                    }
                    // Taken already, or would cross an alignment made here.
                    let taken = made.iter().any(|(ui, _)| *ui == i);
                    let crosses = made.iter().any(|(ui, lj)| (*ui < i) != (*lj < vj));
                    if taken || crosses {
                        continue;
                    }
                    align.insert(u, v);
                    let ru = root[&u];
                    root.insert(v, ru);
                    align.insert(v, ru);
                    made.push((i, vj));
                }
            }
        }
        (root, align)
    }

    /// Pack the blocks as tightly as the boxes allow, leftward or rightward.
    ///
    /// Blocks are placed by longest path over a graph of "this block must sit
    /// right of that one by at least so much", one edge for every pair of
    /// slots side by side in a row; the separation is centre to centre, half
    /// each box plus the gap, so boxes of any width pack correctly. For the
    /// rightward layouts the rows are mirrored, packed, and mirrored back.
    fn horizontal_compaction(
        &self,
        root: &HashMap<Slot, Slot>,
        _align: &HashMap<Slot, Slot>,
        left: bool,
    ) -> HashMap<Slot, i32> {
        // Block graph: for consecutive slots u,v (in the packing direction),
        // root[v] must be at least sep(u,v) right of root[u].
        let mut succ: HashMap<Slot, Vec<(Slot, i32)>> = HashMap::new();
        let mut indeg: HashMap<Slot, usize> = HashMap::new();
        for s in root.keys() {
            indeg.entry(root[s]).or_insert(0);
        }
        for row in &self.rows {
            let seq: Vec<Slot> =
                if left { row.clone() } else { row.iter().rev().copied().collect() };
            for w in seq.windows(2) {
                let (u, v) = (root[&w[0]], root[&w[1]]);
                let sep = self.width[&w[0]] / 2 + self.width[&w[1]] / 2 + gap_between(&w[0], &w[1]);
                let list = succ.entry(u).or_default();
                match list.iter_mut().find(|(t, _)| *t == v) {
                    Some(e) => e.1 = e.1.max(sep),
                    None => {
                        list.push((v, sep));
                        *indeg.entry(v).or_insert(0) += 1;
                    }
                }
            }
        }
        // Longest path from the sources, in a stable order.
        let mut x: HashMap<Slot, i32> = HashMap::new();
        let mut ready: Vec<Slot> =
            indeg.iter().filter(|(_, d)| **d == 0).map(|(s, _)| *s).collect();
        ready.sort_unstable();
        let mut queue: VecDeque<Slot> = ready.into_iter().collect();
        for s in queue.iter() {
            x.insert(*s, 0);
        }
        while let Some(u) = queue.pop_front() {
            let xu = x[&u];
            if let Some(list) = succ.get(&u) {
                let mut list = list.clone();
                list.sort_unstable();
                for (v, sep) in list {
                    let e = x.entry(v).or_insert(i32::MIN);
                    *e = (*e).max(xu + sep);
                    let d = indeg.get_mut(&v).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
        // Every slot at its block's x; mirrored back for a rightward layout.
        let mut out: HashMap<Slot, i32> = HashMap::new();
        for s in root.keys() {
            let c = x[&root[s]];
            out.insert(*s, if left { c } else { -c });
        }
        out
    }

    /// Choose among the four layouts.
    ///
    /// Brandes and Köpf balance the four by giving every slot the average of
    /// the middle two of its four positions. That keeps the order and the
    /// separation, but not the blocks: a slot aligned one way in two layouts
    /// and another way in the other two lands between, and an edge that was
    /// vertical in each is vertical in none. With bends at every corridor
    /// slot that costs a little straightness; with straight lines it puts
    /// the corridor off the line and the line through a box. So one layout
    /// is taken whole: the one with the fewest slanted long edges, and among
    /// those the shortest edges in total.
    fn balance(&mut self, layouts: Vec<HashMap<Slot, i32>>) {
        let score = |l: &HashMap<Slot, i32>| -> (usize, i64) {
            let mut slanted = 0usize;
            let mut length: i64 = 0;
            for (a, b) in &self.links {
                let d = (l[a] - l[b]).abs() as i64;
                length += d;
                if d != 0 && (matches!(a, Slot::Dummy(..)) || matches!(b, Slot::Dummy(..))) {
                    slanted += 1;
                }
            }
            (slanted, length)
        };
        let best = layouts
            .iter()
            .enumerate()
            .min_by_key(|(i, l)| (score(l), *i))
            .map(|(i, _)| i)
            .unwrap_or(0);
        for (s, c) in &layouts[best] {
            self.x.insert(*s, c - self.width[s] / 2);
        }
    }

    /// Size every corridor slot to the width its edge's straight line sweeps
    /// across the row.
    ///
    /// The line from one end's centre to the other's crosses each row it
    /// passes at a slant, and across the height of that row's boxes it moves
    /// sideways by the row height times the slope. A slot the width of a
    /// lane protects a vertical line; a slanted one needs the slot as wide as
    /// its sweep, plus the lane, or the box packed beside the slot sits under
    /// the line. The slot is widened and nothing else: where it goes is still
    /// the ordering's and the placement's call. Pinning it to the line's
    /// crossing point and re-sequencing the row to honour that was tried, and
    /// took a third more crossings for a fifth fewer lines through boxes,
    /// which is the wrong way round.
    fn size_lanes(&mut self) {
        let row_of = self.row_index();
        let tops = self.row_tops();
        let mut widths: Vec<(Slot, i32)> = Vec::new();
        for (ei, (upper, lower)) in &self.lane_ends {
            let (u, l) = (Slot::Item(*upper), Slot::Item(*lower));
            let (Some(&ru), Some(&rl)) = (row_of.get(&u), row_of.get(&l)) else { continue };
            let ux = (self.x[&u] + self.width[&u] / 2) as f64;
            let lx = (self.x[&l] + self.width[&l] / 2) as f64;
            // Slope over the real vertical distance between the two ends.
            let drop = (tops[rl] - tops[ru]).max(1) as f64;
            let per_px = (lx - ux) / drop;
            let sweep = (per_px.abs() * self.pitch as f64).ceil() as i32;
            for seg in 0..(rl - ru - 1) {
                let dm = Slot::Dummy(*ei, seg);
                if self.width.contains_key(&dm) {
                    widths.push((dm, sweep + DUMMY_W));
                }
            }
        }
        for (dm, w) in widths {
            self.width.insert(dm, w);
        }
    }

    /// Grow each row gap until no straight edge cuts through a box in either
    /// of the rows it ends in.
    ///
    /// A line leaves its box through the bottom if it is steep and through
    /// the side if it is shallow, and a shallow one then runs along the row
    /// band through the neighbours before it drops. How shallow is shallow
    /// depends on the gap: the further apart the rows, the steeper every
    /// line. So each gap is the one number that makes every edge across it
    /// clear its rows — computed exactly, not tuned — and capped at six
    /// times the stock gap, so one edge that spans the width of the drawing
    /// costs one tall gap and not an absurd one. Per gap rather than per
    /// drawing: a ten-rank chain with one shallow edge at the bottom should
    /// not be three thousand pixels tall for it.
    ///
    /// A long edge is looked after in the rows it crosses by its corridor,
    /// but its two end rows are like any other edge's: it arrives at its box
    /// at some slant, and the box beside that one is in the way if the slant
    /// is shallow. Its drop is the sum of the gaps it spans, and it is the
    /// gap next to the end row that grows.
    fn fit_gap(&mut self, edges: &[(usize, usize)]) {
        let row_of = self.row_index();
        let mut need = vec![VGAP; self.rows.len()];
        for &(a, b) in edges {
            if a == b {
                continue;
            }
            let (sa, sb) = (Slot::Item(a), Slot::Item(b));
            let (Some(&ra), Some(&rb)) = (row_of.get(&sa), row_of.get(&sb)) else { continue };
            if ra == rb {
                continue;
            }
            let (up, dn) = if ra < rb { (sa, sb) } else { (sb, sa) };
            let (ux, dx) = (self.x[&up] + self.width[&up] / 2, self.x[&dn] + self.width[&dn] / 2);
            let (upper_row, lower_row) = (row_of[&up], row_of[&dn]);
            // Every other box in the two end rows that lies between the ends
            // horizontally: the line must be past the row band by the time
            // it reaches the box's near edge.
            for (row, from_top) in [(upper_row, true), (lower_row, false)] {
                let grows = if from_top { upper_row } else { lower_row - 1 };
                for o in &self.rows[row] {
                    if *o == up || *o == dn || !matches!(o, Slot::Item(_)) {
                        continue;
                    }
                    let (ox0, ox1) = (self.x[o], self.x[o] + self.width[o]);
                    let (lo, hi) = (ux.min(dx), ux.max(dx));
                    if ox1 <= lo || ox0 >= hi {
                        continue;
                    }
                    // Horizontal distance from the line's start (the end in
                    // this row) to the box's near edge, and the total run.
                    let start = if from_top { ux } else { dx };
                    let near = if ox0 > start { ox0 } else { ox1 };
                    let run_to_box = (near - start).abs() as f64;
                    let run = (dx - ux).abs().max(1) as f64;
                    // Half box height plus inset: the line must have dropped
                    // this far by `run_to_box`. Its total drop is the rows
                    // and gaps it spans; whatever is short is added to the
                    // gap by this end.
                    let half = (self.height[o].max(1) as f64) / 2.0 + 2.0;
                    let drop = half * run / run_to_box.max(1.0);
                    let rows = (lower_row - upper_row) as f64;
                    let have: f64 = rows * self.pitch as f64
                        + (upper_row..lower_row).map(|r| need[r] as f64).sum::<f64>();
                    let mut short = (drop - have).ceil() as i32;
                    // The gap by this end first, then the others it spans,
                    // nearest first, each up to the cap.
                    let mut order: Vec<usize> = (upper_row..lower_row).collect();
                    order.sort_by_key(|r| r.abs_diff(grows));
                    for r in order {
                        if short <= 0 {
                            break;
                        }
                        let room = (VGAP * 6 - need[r]).max(0);
                        let add = short.min(room);
                        need[r] += add;
                        short -= add;
                    }
                }
            }
        }
        self.gaps = need.into_iter().map(|g| snap(g.min(VGAP * 6)).max(VGAP)).collect();
    }

    fn row_index(&self) -> HashMap<Slot, usize> {
        self.rows.iter().enumerate().flat_map(|(r, row)| row.iter().map(move |s| (*s, r))).collect()
    }

    /// The gap below row `r`: as fitted, or the stock one.
    fn gap_below(&self, r: usize) -> i32 {
        self.gaps.get(r).copied().unwrap_or(VGAP)
    }

    /// The y of each row's top, from the pitch and the gaps.
    fn row_tops(&self) -> Vec<i32> {
        let mut y = Vec::with_capacity(self.rows.len());
        let mut cursor = 0;
        for r in 0..self.rows.len() {
            y.push(snap(cursor));
            cursor += self.pitch + self.gap_below(r);
        }
        y
    }

    fn assign_y(&mut self) {
        self.y = self.row_tops();
    }

    fn finish(&self, items: &[Item]) -> Vec<Rect> {
        let mut out = vec![Rect::default(); items.len()];
        for (r, row) in self.rows.iter().enumerate() {
            for s in row {
                if let Slot::Item(i) = s {
                    out[*i] = Rect { x: self.x[s], y: self.y[r], w: items[*i].w, h: items[*i].h };
                }
            }
        }
        out
    }
}

/// Does a drawn line pass through any box other than the two it joins?
fn path_is_clear(path: &[Pt], all: &[Rect], a: usize, b: usize) -> bool {
    for (i, other) in all.iter().enumerate() {
        if i == a || i == b || other.w == 0 {
            continue;
        }
        // A small inset, so an edge grazing a corner is not counted as a hit.
        let box_ = Rect { x: other.x + 2, y: other.y + 2, w: other.w - 4, h: other.h - 4 };
        if path.windows(2).any(|s| segment_hits(s[0], s[1], box_)) {
            return false;
        }
    }
    true
}

/// Does the line between two boxes' centres pass through any other box?
fn straight_is_clear(from: Rect, to: Rect, all: &[Rect], a: usize, b: usize) -> bool {
    path_is_clear(&[from.center(), to.center()], all, a, b)
}

/// Does the open segment from `p` to `q` pass through the rectangle?
///
/// Exact, by clipping the segment's parameter range against each of the four
/// half-planes (Liang–Barsky). The previous version sampled forty-eight points
/// along the segment and tested each, which misses a box the segment crosses
/// between two samples — and, because of how the samples round, whether it
/// missed depended on which end the segment was walked from. The same three
/// points were "clear" one way round and through a box the other, and the
/// simplify pass, which reads the path in one direction and the drawing in the
/// other, believed the wrong one.
///
/// Touching only the border does not count: the test is for the interior, and
/// the callers inset the box by two pixels precisely so that a line grazing a
/// corner is not a hit.
fn segment_hits(p: Pt, q: Pt, b: Rect) -> bool {
    if b.w <= 0 || b.h <= 0 {
        return false;
    }
    let (x0, y0, x1, y1) = (p.x as f64, p.y as f64, q.x as f64, q.y as f64);
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (left, right) = (b.x as f64, (b.x + b.w) as f64);
    let (top, bottom) = (b.y as f64, (b.y + b.h) as f64);

    // The segment is x0 + t·dx for t in [0, 1]. Each edge of the box cuts
    // that range down; if it is still non-empty at the end, the segment enters
    // the box.
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (num, den) in [
        (x0 - left, -dx),  // x >= left
        (right - x0, dx),  // x <= right
        (y0 - top, -dy),   // y >= top
        (bottom - y0, dy), // y <= bottom
    ] {
        if den == 0.0 {
            // Parallel to this edge: outside the slab entirely, or fine.
            if num < 0.0 {
                return false;
            }
        } else {
            let t = num / den;
            if den < 0.0 {
                // Entering.
                if t > t1 {
                    return false;
                }
                if t > t0 {
                    t0 = t;
                }
            } else {
                // Leaving.
                if t < t0 {
                    return false;
                }
                if t < t1 {
                    t1 = t;
                }
            }
        }
    }
    // Strictly inside for some stretch of the segment, not just a corner.
    t0 < t1
}

/// Number of pairs `(i, j)` with `i < j` and `v[i] > v[j]`, by merge sort.
fn inversions(v: &[usize]) -> usize {
    fn go(v: &mut [usize], buf: &mut Vec<usize>) -> usize {
        let n = v.len();
        if n < 2 {
            return 0;
        }
        let mid = n / 2;
        let mut count = go(&mut v[..mid], buf) + go(&mut v[mid..], buf);
        buf.clear();
        let (mut i, mut j) = (0, mid);
        while i < mid && j < n {
            if v[i] <= v[j] {
                buf.push(v[i]);
                i += 1;
            } else {
                // v[j] jumps ahead of everything left in the first half.
                count += mid - i;
                buf.push(v[j]);
                j += 1;
            }
        }
        buf.extend_from_slice(&v[i..mid]);
        buf.extend_from_slice(&v[j..]);
        v.copy_from_slice(buf);
        count
    }
    let mut v = v.to_vec();
    let mut buf = Vec::with_capacity(v.len());
    go(&mut v, &mut buf)
}

fn slot_key(items: &[Item], s: Slot) -> (u8, String, String) {
    match s {
        Slot::Item(i) => (0, items[i].name.clone(), items[i].id.clone()),
        // Dummies sort after real nodes, and among themselves by the edge they
        // belong to, so ordering stays reproducible.
        Slot::Dummy(e, seg) => (1, format!("{e:08}"), format!("{seg:04}")),
    }
}

/// Somewhere to put a new object without disturbing anything already placed.
///
/// This is what makes pinning the default: existing objects stay where they
/// are, so adding one element does not produce a four-hundred-line diff.
pub fn free_slot(taken: &[Rect], w: i32, h: i32) -> Rect {
    for row in 0..500 {
        for col in 0..40 {
            let candidate = Rect { x: col * (w + HGAP), y: row * (h + VGAP), w, h };
            let clash = taken.iter().any(|t| {
                candidate.x < t.x + t.w + HGAP / 2
                    && t.x < candidate.x + candidate.w + HGAP / 2
                    && candidate.y < t.y + t.h + VGAP / 2
                    && t.y < candidate.y + candidate.h + VGAP / 2
            });
            if !clash {
                return candidate;
            }
        }
    }
    Rect { x: 0, y: 0, w, h }
}

#[cfg(test)]
mod tests {

    /// The label a box gets has to fit the room Archi leaves for it. This is
    /// the check the old flat seven-pixels-a-character estimate failed: on a
    /// real model thirty-five names were drawn wider than their box.
    fn clipped(label: &str) -> bool {
        let (w, h) = fit_size(label);
        let usable = w - 2 * ICON_INSET;
        // A word wider than the line is clipped whatever the wrap does with
        // the rest, and lines that do not fit the height are cut off.
        label.split_whitespace().any(|word| text_px(word) > usable)
            || wrapped_lines(label, usable) * 15 + 10 > h
    }

    #[test]
    fn every_name_fits_the_box_it_is_given() {
        for label in [
            // The one that started it: a 120 box leaves 66 pixels of text and
            // "Communications" alone is 88.
            "Communications Providers",
            "Underwriting Not India-Calibrated",
            "Purpose-Bound, Withdrawable Consent",
            "Money Moves Only Between RE and Borrower Accounts",
            "Launch Window — Disbursements from March 2027",
            "Daily Three-Way Reconciliation",
            "API",
            "Payment API",
            "МФО и заёмщик",
            "",
        ] {
            assert!(
                !clipped(label),
                "{label:?} does not fit fit_size({label:?}) = {:?}",
                fit_size(label)
            );
        }
    }

    #[test]
    fn a_word_wider_than_the_cap_widens_the_box_past_it() {
        // Word wrap cannot break inside a word, so a cap that clipped one
        // would be a cap that clipped the name.
        let (w, _) = fit_size("Kraftfahrzeughaftpflichtversicherungsvertrag");
        assert!(w > 264, "capped at {w}, which would clip the word");
        assert!(!clipped("Kraftfahrzeughaftpflichtversicherungsvertrag"));
    }

    #[test]
    fn a_short_name_still_gets_the_stock_box() {
        assert_eq!(fit_size("Refunds"), (120, 55));
        assert_eq!(fit_size("Payment API"), (120, 55));
    }

    #[test]
    fn a_note_is_not_charged_for_an_icon_it_does_not_have() {
        let label = "Reporting to All Four CICs Under RE Membership";
        let (element, _) = fit_size(label);
        let (note, _) = fit_note_size(label);
        assert!(note < element, "note {note} is not narrower than element {element}");
    }

    #[test]
    fn sizes_land_on_the_grid_and_never_shrink_to_get_there() {
        for label in ["Communications Providers", "Adapter Checklist Before a Provider Goes Live"] {
            let (w, h) = fit_size(label);
            assert_eq!(w % GRID, 0, "{label:?} width {w} is off the grid");
            assert!(h >= 55, "{label:?} height {h} is under the stock box");
        }
    }

    use super::*;

    fn items(n: usize) -> Vec<Item> {
        (0..n)
            .map(|i| Item { id: format!("id{i}"), name: format!("Node {i}"), w: 120, h: 55 })
            .collect()
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    /// Does the straight line between two boxes pass through a third?
    fn cuts_through(from: Rect, to: Rect, other: Rect) -> bool {
        let (x1, y1) = (from.x + from.w / 2, from.y + from.h / 2);
        let (x2, y2) = (to.x + to.w / 2, to.y + to.h / 2);
        for step in 1..40 {
            let t = step as f64 / 40.0;
            let p = Pt {
                x: (x1 as f64 + (x2 - x1) as f64 * t) as i32,
                y: (y1 as f64 + (y2 - y1) as f64 * t) as i32,
            };
            if other.contains(p) {
                return true;
            }
        }
        false
    }

    /// The case that prompted all of this: a chain inside one ArchiMate layer.
    /// Ranking by layer puts every box in one row and every edge through its
    /// neighbours; ranking by dependency does not.
    #[test]
    fn a_chain_within_one_layer_becomes_a_column_not_a_row() {
        let it: Vec<Item> = ["Payment API", "Authorize", "Card Auth", "Payment Record"]
            .iter()
            .enumerate()
            .map(|(i, n)| Item { id: format!("i{i}"), name: n.to_string(), w: 120, h: 55 })
            .collect();
        let edges = vec![(0, 1), (1, 2), (1, 3)];

        let p = place(&it, &edges, Algorithm::Sugiyama);
        assert!(p.rects[0].y < p.rects[1].y, "the component sits above the function");
        assert!(p.rects[1].y < p.rects[2].y, "and the function above what it realizes");

        for (a, b) in &edges {
            for (k, other) in p.rects.iter().enumerate() {
                if k == *a || k == *b {
                    continue;
                }
                assert!(
                    !cuts_through(p.rects[*a], p.rects[*b], *other),
                    "edge {a}->{b} runs through box {k}"
                );
            }
        }
    }

    /// The line an edge is drawn as: centre to centre. There are no bends.
    fn drawn_path(p: &Placement, edges: &[(usize, usize)], ei: usize) -> Vec<Pt> {
        let (a, b) = edges[ei];
        vec![p.rects[a].center(), p.rects[b].center()]
    }

    /// A long edge skips a rank, so by construction something shares that rank.
    /// Edges are straight lines, so the only way past is for the placement to
    /// keep the rank's boxes off the line: the corridor reserves a slot on
    /// the way, and the boxes pack around it.
    #[test]
    fn a_long_edge_never_crosses_the_rank_it_skips() {
        // 0 -> 1 -> 2 with an extra 0 -> 2 that skips a rank. The lane pushes
        // box 1 aside and the edge runs straight past it.
        let it = items(3);
        let edges = vec![(0, 1), (1, 2), (0, 2)];
        let p = place(&it, &edges, Algorithm::Sugiyama);
        let path = drawn_path(&p, &edges, 2);
        assert!(
            !path.windows(2).any(|s| segment_hits(s[0], s[1], p.rects[1])),
            "the skipping edge crosses box 1: {path:?} vs {:?}",
            p.rects[1]
        );

        // Crowd the middle rank: the corridor still gets its slot among the
        // six, and the line still passes through it and nothing else.
        let it = items(8);
        let mut edges = vec![(0, 1), (1, 7), (0, 7)];
        for i in 2..7 {
            edges.push((0, i));
            edges.push((i, 7));
        }
        let p = place(&it, &edges, Algorithm::Sugiyama);
        let path = drawn_path(&p, &edges, 2);
        for (k, other) in p.rects.iter().enumerate() {
            if k == 0 || k == 7 {
                continue;
            }
            assert!(
                !path.windows(2).any(|s| segment_hits(s[0], s[1], *other)),
                "the long edge crosses box {k}: {path:?} vs {other:?}"
            );
        }
    }

    #[test]
    fn a_cycle_does_not_hang_or_collapse() {
        let it = items(3);
        let edges = vec![(0, 1), (1, 2), (2, 0)];
        let p = place(&it, &edges, Algorithm::Sugiyama);
        let ys: HashSet<i32> = p.rects.iter().map(|r| r.y).collect();
        assert!(ys.len() > 1, "a cycle still gets laid out over several rows");
    }

    #[test]
    fn fewer_crossings_than_the_naive_ordering() {
        // A deliberate tangle: every node in the top row points at the opposite
        // node in the bottom row.
        let n = 6;
        let it: Vec<Item> = (0..n)
            .map(|i| Item { id: format!("i{i}"), name: format!("N{i}"), w: 120, h: 55 })
            .collect();
        let edges: Vec<(usize, usize)> = (0..n / 2).map(|i| (i, n - 1 - i)).collect();

        let p = place(&it, &edges, Algorithm::Sugiyama);
        // Each pair should end up vertically aligned, which is zero crossings.
        for (a, b) in &edges {
            let ca = p.rects[*a].x + p.rects[*a].w / 2;
            let cb = p.rects[*b].x + p.rects[*b].w / 2;
            assert!((ca - cb).abs() <= GRID, "edge {a}->{b} is not vertical: {ca} vs {cb}");
        }
    }

    #[test]
    fn layout_is_reproducible_and_order_independent() {
        let it = items(9);
        let edges = vec![(0, 1), (1, 2), (3, 4), (0, 5), (6, 7)];
        for algo in [Algorithm::Grid, Algorithm::Sugiyama] {
            assert_eq!(place(&it, &edges, algo).rects, place(&it, &edges, algo).rects, "{algo:?}");
        }

        // Reversing the input must not move anything.
        let a = place(&it, &edges, Algorithm::Sugiyama).rects;
        let mut reversed = it.clone();
        reversed.reverse();
        let remap = |i: usize| it.len() - 1 - i;
        let redges: Vec<(usize, usize)> =
            edges.iter().map(|(x, y)| (remap(*x), remap(*y))).collect();
        let b = place(&reversed, &redges, Algorithm::Sugiyama).rects;
        for (i, item) in it.iter().enumerate() {
            let j = reversed.iter().position(|r| r.id == item.id).unwrap();
            assert_eq!(a[i], b[j], "{} moved", item.id);
        }
    }

    #[test]
    fn nothing_overlaps_and_everything_is_on_the_grid() {
        let it = items(12);
        let edges = vec![(0, 1), (1, 2), (2, 3), (0, 4), (5, 6), (7, 8), (2, 9)];
        for algo in [Algorithm::Grid, Algorithm::Sugiyama] {
            let p = place(&it, &edges, algo);
            for r in &p.rects {
                assert_eq!(r.x % GRID, 0, "{algo:?} {r:?}");
                assert_eq!(r.y % GRID, 0, "{algo:?} {r:?}");
            }
            for (i, a) in p.rects.iter().enumerate() {
                for b in p.rects.iter().skip(i + 1) {
                    assert!(!overlaps(*a, *b), "{algo:?}: {a:?} overlaps {b:?}");
                }
            }
        }
    }

    /// The reported case: a wide, shallow graph. Layering has nothing to stack,
    /// so the rank would be one enormous row — it gets folded onto several
    /// lines instead, and the drawing stays readable without giving up the
    /// ranking.
    #[test]
    fn a_wide_rank_is_folded_rather_than_drawn_off_the_page() {
        let it = items(60);
        // Two ranks, sixty nodes: every edge goes from one of the first thirty
        // to its partner in the second thirty.
        let edges: Vec<(usize, usize)> = (0..30).map(|i| (i, i + 30)).collect();

        let layered = place(&it, &edges, Algorithm::Sugiyama);
        assert_eq!(layered.algorithm, Algorithm::Sugiyama);
        // The fold judges rows packed end to end; placement then spreads a
        // row to line its slots up with their neighbours, and on a folded
        // pair whose lines link across each other that spread is real. So the
        // bound holds with a little slack, not exactly — what matters is that
        // it is nowhere near the fifteen-to-one the unfolded row came out at.
        assert!(
            wideness(&layered.rects) <= MAX_WIDTH_RATIO * 1.25,
            "the fold should have brought it near the bound, got {:?}",
            wideness(&layered.rects)
        );

        // Thirty pairs are thirty components, each drawn on its own and then
        // packed on shelves — so a source is above *its* target, not above
        // every target on a lower shelf. That is the ranking that matters,
        // and it is what a grid would throw away.
        for i in 0..30 {
            assert!(
                layered.rects[i].y < layered.rects[i + 30].y,
                "pair {i}: source not above its target"
            );
            assert_eq!(
                layered.rects[i].x,
                layered.rects[i + 30].x,
                "pair {i}: not a vertical pair"
            );
        }

        // `auto` keeps the layering: a grid would be squarer but crosses the
        // thirty edges of a perfect matching where the layering crosses none.
        assert_eq!(place(&it, &edges, Algorithm::Auto).algorithm, Algorithm::Sugiyama);

        // A deep chain is tall and narrow, which is what a layered drawing of a
        // chain should be — neither the fold nor the fallback may touch it.
        let deep: Vec<(usize, usize)> = (0..11).map(|i| (i, i + 1)).collect();
        let tall = place(&items(12), &deep, Algorithm::Auto);
        assert_eq!(tall.algorithm, Algorithm::Sugiyama, "a chain still gets layered");
        assert_eq!(tall.rects.len(), 12);
        let rows: HashSet<i32> = tall.rects.iter().map(|r| r.y).collect();
        assert_eq!(rows.len(), 12, "a chain is one node per row, unfolded");
    }

    /// Components are laid out apart and packed, so nothing can leave a hole
    /// between them. Laid out together, one component drifted off to the right
    /// during the placement sweeps and — with no edge crossing the gap for the
    /// compaction to shorten — was never pulled back, leaving two thousand
    /// pixels of nothing in a fourteen-box drawing.
    #[test]
    fn components_are_packed_without_a_hole_between_them() {
        // Three components of different shapes: a fan of three into one, a
        // chain of four, and a lone pair. Names chosen so the by-name order
        // interleaves them, which is what let the drift happen.
        let names = [
            "Alpha", "Bravo", "Charlie", "Delta", // fan: 0,1,2 -> 3
            "Echo", "Foxtrot", "Golf", "Hotel", // chain: 4 -> 5 -> 6 -> 7
            "India", "Juliet", // pair: 8 -> 9
        ];
        let it: Vec<Item> = names
            .iter()
            .enumerate()
            .map(|(i, n)| Item { id: format!("i{i}"), name: n.to_string(), w: 120, h: 55 })
            .collect();
        let edges = vec![(0, 3), (1, 3), (2, 3), (4, 5), (5, 6), (6, 7), (8, 9)];
        let p = place(&it, &edges, Algorithm::Sugiyama);

        // No horizontal gap between consecutive boxes wider than a box.
        let mut spans: Vec<(i32, i32)> = p.rects.iter().map(|r| (r.x, r.x + r.w)).collect();
        spans.sort_unstable();
        let mut reach = spans[0].1;
        for (x0, x1) in &spans[1..] {
            assert!(x0 - reach <= 120 + HGAP, "a hole of {} px before x={x0}", x0 - reach);
            reach = reach.max(*x1);
        }

        // Each component keeps its own shape: the pair is vertical, the chain
        // is a column, the fan's three sources sit above its sink.
        assert_eq!(p.rects[8].x, p.rects[9].x, "the pair is a vertical pair");
        assert!(p.rects[8].y < p.rects[9].y);
        for w in [(4, 5), (5, 6), (6, 7)] {
            assert!(p.rects[w.0].y < p.rects[w.1].y, "chain {w:?} is a column");
        }
        for src in 0..3 {
            assert!(p.rects[src].y < p.rects[3].y, "fan source {src} above the sink");
        }

        // And rows line up across components: every y is on the same pitch.
        let mut ys: Vec<i32> = p.rects.iter().map(|r| r.y).collect();
        ys.sort_unstable();
        ys.dedup();
        assert!(ys.len() <= 4, "components share rows rather than each taking its own: {ys:?}");
    }

    /// The fold is bounded by width, so it must not fire on a row that fits —
    /// otherwise every ordinary diagram would start stacking for no reason.
    #[test]
    fn a_row_that_fits_is_left_alone() {
        let it = items(8);
        let edges: Vec<(usize, usize)> = (0..4).map(|i| (i, i + 4)).collect();
        let p = place(&it, &edges, Algorithm::Sugiyama);
        let rows: HashSet<i32> = p.rects.iter().map(|r| r.y).collect();
        assert_eq!(rows.len(), 2, "eight boxes in two ranks stay in two rows");
    }

    /// Network simplex must return a proper layering — every edge at least one
    /// rank long, pointing down — and it must never be longer in total than
    /// the longest-path layering it started from. Over many shapes, because a
    /// pivot with the sign wrong the other way would still "converge" and
    /// still look like a drawing.
    #[test]
    fn network_simplex_shortens_without_breaking_the_layering() {
        let mut seed = 777u64;
        let mut rnd = |m: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % m.max(1)
        };
        let mut shortened = 0;
        for trial in 0..300 {
            let n = 3 + trial % 25;
            // A DAG by construction: every edge from a lower to a higher index.
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for _ in 0..(n + trial % 15) {
                let a = rnd(n);
                let b = rnd(n);
                if a < b {
                    edges.push((a, b));
                } else if b < a {
                    edges.push((b, a));
                }
            }
            if edges.is_empty() {
                continue;
            }
            // Longest path, as rank_by_dependency computes it.
            let mut lp = vec![0usize; n];
            for _ in 0..n {
                for &(a, b) in &edges {
                    lp[b] = lp[b].max(lp[a] + 1);
                }
            }
            let span = |r: &[usize]| -> usize { edges.iter().map(|&(a, b)| r[b] - r[a]).sum() };
            let before = span(&lp);

            let ns = network_simplex(n, &edges, lp.clone());
            for &(a, b) in &edges {
                assert!(
                    ns[b] > ns[a],
                    "trial {trial}: edge {a}->{b} is not pointing down: {} -> {}",
                    ns[a],
                    ns[b]
                );
            }
            let after = span(&ns);
            assert!(
                after <= before,
                "trial {trial}: simplex lengthened the drawing {before} -> {after}"
            );
            if after < before {
                shortened += 1;
            }
        }
        assert!(
            shortened > 30,
            "simplex shortened only {shortened} of 300 graphs; it is not doing anything"
        );
    }

    /// The reported shape: a hub reached by one long chain and by many single
    /// hops. Longest path anchors the hub at the chain's end and stretches
    /// every hop; the simplex lifts the hub to the hops and stretches only the
    /// chain.
    #[test]
    fn a_hub_at_the_end_of_a_chain_is_lifted_toward_its_crowd() {
        // 0 -> 1 -> 2 -> 3 -> 4 -> 5 -> hub(6); leaves 7..27 -> hub.
        let n = 28;
        let mut edges: Vec<(usize, usize)> = (0..6).map(|i| (i, i + 1)).collect();
        for leaf in 7..n {
            edges.push((leaf, 6));
        }
        let mut lp = vec![0usize; n];
        for _ in 0..n {
            for &(a, b) in &edges {
                lp[b] = lp[b].max(lp[a] + 1);
            }
        }
        assert_eq!(lp[6], 6, "longest path puts the hub six deep");
        assert!(lp[7] == 0, "and every leaf at the top, six ranks from it");

        let ns = network_simplex(n, &edges, lp);
        for leaf in 7..n {
            assert_eq!(ns[6] - ns[leaf], 1, "leaf {leaf} should be one rank above the hub");
        }
    }

    /// Straight lines and boxes, over a spread of shapes rather than one
    /// hand-picked graph.
    ///
    /// Two things are asserted. An edge between adjacent rows never cuts a
    /// box in either of its rows: the row gap is chosen so that every such
    /// line has dropped clear of the row band by the time it reaches a
    /// neighbour, and that is exact, up to the gap's cap. And the share of
    /// long edges — two or more ranks — drawn through a box in a rank they
    /// skip stays under a bound. It is not zero: a slanted line across a
    /// crowded rank needs a slot the width of its slant, and the ordering
    /// does not always leave one where the line runs; forcing it to did so
    /// at a third more crossings, which is the wrong trade. The bound is a
    /// ratchet — tighten it as the placement improves, never loosen it.
    #[test]
    fn straight_edges_clear_their_own_rows_and_mostly_the_ranks_they_skip() {
        // A fixed sequence, so a failure here is reproducible rather than
        // something that shows up one run in ten.
        let mut seed = 12345u64;
        let mut rnd = |m: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % m.max(1)
        };

        let (mut adjacent, mut long, mut long_through) = (0usize, 0usize, 0usize);
        for trial in 0..400 {
            let n = 4 + trial % 20;
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for _ in 0..(n + trial % 12) {
                let (a, b) = (rnd(n), rnd(n));
                if a != b {
                    edges.push((a, b));
                }
            }
            if edges.is_empty() {
                continue;
            }

            let p = place(&items(n), &edges, Algorithm::Sugiyama);
            let mut ys: Vec<i32> = p.rects.iter().map(|r| r.y).collect();
            ys.sort_unstable();
            ys.dedup();
            let row = |y: i32| ys.iter().position(|v| *v == y).unwrap();

            for (ei, (a, b)) in edges.iter().enumerate() {
                if a == b {
                    continue;
                }
                let path = drawn_path(&p, &edges, ei);
                let clear = path_is_clear(&path, &p.rects, *a, *b);
                if row(p.rects[*a].y).abs_diff(row(p.rects[*b].y)) <= 1 {
                    adjacent += 1;
                    assert!(
                        clear,
                        "trial {trial}: adjacent edge {ei} ({a} -> {b}) cuts a box in its own row: {path:?}"
                    );
                } else {
                    long += 1;
                    if !clear {
                        long_through += 1;
                    }
                }
            }
        }
        assert!(adjacent > 3000 && long > 100, "the sweep proves little: {adjacent} / {long}");
        let share = long_through as f64 / long as f64;
        eprintln!("long edges through a box: {long_through} of {long} ({share:.3})");
        assert!(share < 0.10, "{long_through} of {long} long edges through a box ({share:.2})");
    }

    /// Crossings and lines through boxes in a placement, as the user sees it.
    fn tangle_count(p: &Placement, edges: &[(usize, usize)]) -> (usize, usize) {
        let through = edges
            .iter()
            .filter(|(a, b)| {
                a != b && !straight_is_clear(p.rects[*a], p.rects[*b], &p.rects, *a, *b)
            })
            .count();
        (drawn_crossings(p, edges), through)
    }

    /// The reported case: a hub with thirteen leaves was folded into two
    /// lines under it, and three of the far line's edges ran through the
    /// near line's boxes. Direction is not what the drawing is for: the
    /// leaves go on both sides of the hub, half above and half below, and
    /// nothing crosses anything.
    #[test]
    fn a_hub_and_its_fan_sit_on_both_sides_of_it() {
        let it = items(15);
        let edges: Vec<(usize, usize)> = (1..15).map(|i| (0, i)).collect();
        let p = place(&it, &edges, Algorithm::Sugiyama);
        assert_eq!(tangle_count(&p, &edges), (0, 0));
        let hub = p.rects[0].y;
        let above = p.rects[1..].iter().filter(|r| r.y < hub).count();
        let below = p.rects[1..].iter().filter(|r| r.y > hub).count();
        assert_eq!((above, below), (7, 7), "the fan splits evenly around the hub");
        assert!(wideness(&p.rects) <= MAX_WIDTH_RATIO, "{}", wideness(&p.rects));
        // And the hub stands over the middle of each half, not at its end.
        let centre = |r: &Rect| r.x + r.w / 2;
        let (lo, hi) = (
            p.rects[1..].iter().map(centre).min().unwrap(),
            p.rects[1..].iter().map(centre).max().unwrap(),
        );
        let mid = (lo + hi) / 2;
        assert!(
            (centre(&p.rects[0]) - mid).abs() <= 2 * GRID,
            "hub at {}, fan spans {lo}..{hi}",
            centre(&p.rects[0])
        );
    }

    /// Two hubs sharing a crowd — a K(2,n) — cross about n² times with both
    /// hubs above the crowd, and never with one above and one below. The
    /// arrows all point the same way; the drawing does not have to.
    #[test]
    fn two_hubs_sharing_a_crowd_take_opposite_sides_of_it() {
        let it = items(8);
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for leaf in 2..8 {
            edges.push((0, leaf));
            edges.push((1, leaf));
        }
        let p = place(&it, &edges, Algorithm::Sugiyama);
        assert_eq!(tangle_count(&p, &edges), (0, 0));
        let crowd: HashSet<i32> = p.rects[2..].iter().map(|r| r.y).collect();
        assert_eq!(crowd.len(), 1, "the crowd is one row");
        let row = *crowd.iter().next().unwrap();
        assert!(
            (p.rects[0].y < row) != (p.rects[1].y < row),
            "the hubs take opposite sides: {} / {row} / {}",
            p.rects[0].y,
            p.rects[1].y
        );
    }

    /// A chain hung from a hub — a value stream and the lifecycle that
    /// composes it — lies along one row, in order, and the hub sits over
    /// the middle of it. Ranked by the arrows the chain would be a
    /// staircase and the hub's fan would cross every step.
    #[test]
    fn a_chain_hung_from_a_hub_lies_along_one_row() {
        let it = items(8);
        let mut edges: Vec<(usize, usize)> = (1..8).map(|i| (0, i)).collect();
        edges.extend((1..7).map(|i| (i, i + 1)));
        let p = place(&it, &edges, Algorithm::Sugiyama);
        assert_eq!(tangle_count(&p, &edges), (0, 0));
        let ys: HashSet<i32> = p.rects[1..].iter().map(|r| r.y).collect();
        assert_eq!(ys.len(), 1, "the chain is one row: {ys:?}");
        let xs: Vec<i32> = p.rects[1..].iter().map(|r| r.x).collect();
        let in_order = xs.windows(2).all(|w| w[0] < w[1]) || xs.windows(2).all(|w| w[0] > w[1]);
        assert!(in_order, "the chain is in order along the row: {xs:?}");
    }

    /// A fan too wide for one row on each side is folded, and the fold
    /// nests: the far line's boxes stand between the near line's, so their
    /// edges pass between the near boxes and not through them.
    #[test]
    fn a_folded_fan_nests_and_stays_clean() {
        let it = items(41);
        let edges: Vec<(usize, usize)> = (1..41).map(|i| (0, i)).collect();
        let p = place(&it, &edges, Algorithm::Sugiyama);
        let rows: HashSet<i32> = p.rects.iter().map(|r| r.y).collect();
        assert!(rows.len() >= 5, "forty leaves do not fit on two rows: {} rows", rows.len());
        assert_eq!(tangle_count(&p, &edges), (0, 0));
        assert!(wideness(&p.rects) <= MAX_WIDTH_RATIO * 1.25, "{}", wideness(&p.rects));
    }

    #[test]
    fn a_free_slot_avoids_what_is_already_placed() {
        let taken = vec![Rect { x: 0, y: 0, w: 120, h: 55 }, Rect { x: 200, y: 0, w: 120, h: 55 }];
        let slot = free_slot(&taken, 120, 55);
        for t in &taken {
            assert!(!overlaps(slot, *t), "{slot:?} clashes with {t:?}");
        }
    }
}
