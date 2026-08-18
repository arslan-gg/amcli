//! Placing concepts on a new view.
//!
//! Rows come from the dependency graph and from nothing else. The ArchiMate
//! layer is deliberately not consulted: most relationships in a real model run
//! *within* a layer, so ranking by layer puts them all in one row and turns each
//! into a horizontal line slicing through whatever sits between its ends.
//!
//! This is Sugiyama's method as Gansner et al. describe it for `dot`, and the
//! stages are the ones from that paper. Six things do the work.
//!
//! **Ranking by network simplex.** Longest-path layering is a valid start but
//! shoves every node as high as it can go and anchors every sink wherever the
//! longest chain into it ends, so a hub reached by one long chain and thirty
//! single hops sits far below the thirty. The simplex moves whole subtrees to
//! minimise total edge length, so most edges span exactly one row.
//!
//! **Folding a rank that will not fit.** A hundred motivation elements two
//! ranks deep give layering nothing to stack, and the rank runs off the side of
//! any screen. Such a rank is folded onto several lines, which keeps the
//! ranking that a fallback to a grid would throw away.
//!
//! **Ordering by median, transpose and sifting.** Within each row, slots are put
//! at the median of what they link to; adjacent pairs are then swapped wherever
//! that uncrosses something; and once that stalls, each slot is tried at every
//! position in its row. The three together get within a few per cent of what a
//! far more expensive search finds on real views.
//!
//! **Placing by priority.** Within a row, the slot with the most links to the
//! row it is aligned against is placed first at its ideal, and the rest fit
//! around it — so a hub sits on its column and its edges hang straight, rather
//! than yielding to whatever leaf happened to be on its left. Corridors go
//! first of all, so a long edge comes out straight.
//!
//! **Straight lines, and lanes to keep them clear.** Every edge is one
//! straight line from centre to centre; there are no bendpoints. An edge
//! crossing several rows reserves a corridor in each — sequenced where the
//! line will run and as wide as its slant across the row — and the boxes
//! pack around it. An edge between neighbouring rows is kept off its own
//! row's boxes by the row gap, which is chosen so that every such line has
//! dropped clear of the row band before it reaches a neighbour: one number
//! per drawing, computed rather than tuned. What is left is a slanted long
//! edge across a crowded rank whose corridor the ordering could not seat
//! where the line runs without more crossings than it saves; those are drawn
//! through, and counted.
//!
//! What this cannot do is make a dense graph sparse. A rank of thirty
//! requirements over thirty drivers with seventy-five edges between them has a
//! crossing number in the low hundreds however it is ordered, and most of the
//! crossings that remain on a real model touch a handful of hubs with twenty
//! or more edges each. That is the model, not the drawing.
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
/// How many box-neighbours a corridor-neighbour outweighs when a box decides
/// where to sit. See [`Layered::wish`].
const LANE_VOTES: usize = 3;

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
    /// Rows by dependency, straight lines wherever they fit.
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

/// A box sized so its label fits.
///
/// Archi draws the label inside the box, wrapped by word, at its default 9pt
/// font — around seven pixels a character and fifteen a line, inside five
/// pixels of padding each side. The stock 120×55 box holds three lines of
/// fifteen characters, which is enough for a name of twenty-odd characters
/// and not for one of forty; on a real model the median name was twenty-six
/// characters and one in ten was over thirty-eight, and those were cut off.
///
/// The width is chosen so the name wraps into two lines, from the stock 120
/// up to 264 — a little over two boxes — and the height grows to three lines
/// only if two will not do at that width. Word wrap breaks at spaces, so the
/// estimate is padded by the longest word: a line cannot be narrower than
/// that. Everything is snapped to the grid.
pub fn fit_size(label: &str) -> (i32, i32) {
    const CHAR_W: i32 = 7;
    const LINE_H: i32 = 15;
    const PAD_W: i32 = 10;
    const PAD_H: i32 = 10;
    const MIN_W: i32 = 120;
    const MAX_W: i32 = 264;
    const MIN_H: i32 = 55;

    let chars = label.chars().count() as i32;
    let longest_word =
        label.split_whitespace().map(|w| w.chars().count() as i32).max().unwrap_or(0);

    // Two lines' worth of characters, no narrower than the longest word.
    let per_line = ((chars + 1) / 2).max(longest_word);
    let want = per_line * CHAR_W + PAD_W;
    let w = snap(want.clamp(MIN_W, MAX_W));

    // At that width, how many lines does it actually take? Word wrap loses
    // the tail of most lines, so allow one more than the arithmetic says
    // whenever it wraps at all. Three fit in the stock height; past that the
    // box grows a line at a time.
    let cpl = ((w - PAD_W) / CHAR_W).max(1);
    let lines = (chars + cpl - 1) / cpl + i32::from(chars > cpl);
    let h = if lines <= 3 { MIN_H } else { snap(lines * LINE_H + PAD_H) };
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
            if ratio <= MAX_WIDTH_RATIO {
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

/// How wide a row runs once its slots are laid end to end.
fn row_width(row: &[Slot], width: &HashMap<Slot, i32>) -> i32 {
    row.iter().map(|s| width[s] + HGAP).sum::<i32>().saturating_sub(HGAP).max(0)
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

/// Break rows wider than `budget` into lines, leaving the rest untouched.
///
/// Returns the new rows, and the indices of those that are lines of a folded
/// rank rather than ranks in their own right.
///
/// This runs before any corridor exists, so every row is free to fold; the
/// corridors are laid afterwards, by row, and thread through the lines like
/// any other row.
fn fold_rows(
    rows: &[Vec<Slot>],
    budget: i32,
    width: &HashMap<Slot, i32>,
) -> (Vec<Vec<Slot>>, HashSet<usize>) {
    let mut out: Vec<Vec<Slot>> = Vec::with_capacity(rows.len());
    let mut folded: HashSet<usize> = HashSet::new();
    for row in rows {
        if row.len() < 2 || row_width(row, width) <= budget {
            out.push(row.clone());
            continue;
        }
        // Take the number of lines first and the share per line from it, so the
        // fold comes out balanced. Filling each line to the budget instead
        // would leave the last one holding a single box.
        let budget = budget.max(1);
        let lines = ((row_width(row, width) + budget - 1) / budget) as usize;
        let per = row.len().div_ceil(lines.max(1)).max(1);
        for chunk in row.chunks(per) {
            folded.insert(out.len());
            out.push(chunk.to_vec());
        }
    }
    (out, folded)
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
fn sugiyama_connected(items: &[Item], edges: &[(usize, usize)], min_pitch: i32) -> Placement {
    let (raw_rank, reversed) = rank_by_dependency(items.len(), edges);
    let ranks = compact(&raw_rank);
    let g = build(items, edges, &ranks, &reversed.iter().copied().collect::<Vec<_>>(), min_pitch);
    let rects = g.finish(items);
    Placement { rects, algorithm: Algorithm::Sugiyama }
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
    /// Rows that are lines of a folded rank rather than ranks in their own
    /// right. See [`Self::assign_x`] for why they are held still.
    folded: HashSet<usize>,
    /// The height every row is given. At least the tallest box here, and
    /// when this is one component of several, the tallest box in any of
    /// them — so rows line up across the packed drawing.
    pitch: i32,
    /// The gap between rows. Starts at [`VGAP`] and grows if a straight
    /// edge between adjacent rows would otherwise cut through a box in its
    /// own row — see [`Self::fit_gap`].
    gap: i32,
    /// The two boxes each corridor belongs to, upper end first.
    lane_ends: HashMap<usize, (usize, usize)>,
}

fn build(
    items: &[Item],
    edges: &[(usize, usize)],
    ranks: &[usize],
    reversed: &[usize],
    min_pitch: i32,
) -> Layered {
    let depth = ranks.iter().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<Slot>> = vec![Vec::new(); depth];
    let mut width = HashMap::new();
    let mut height = HashMap::new();

    for (i, item) in items.iter().enumerate() {
        rows[ranks[i]].push(Slot::Item(i));
        width.insert(Slot::Item(i), item.w);
        height.insert(Slot::Item(i), item.h);
    }

    // The edges as they will be drawn: every one pointing down, self-loops
    // out. Cycle-breaking reversed some, and a reversed edge is built the
    // other way up so its corridor descends like everyone else's.
    let drawn: Vec<Option<(usize, usize)>> = edges
        .iter()
        .enumerate()
        .map(|(ei, (a, b))| {
            let (a, b) = if reversed.contains(&ei) { (*b, *a) } else { (*a, *b) };
            (a != b).then_some((a, b))
        })
        .collect();

    let mut g = Layered {
        rows,
        width,
        height,
        links: drawn.iter().flatten().map(|&(a, b)| (Slot::Item(a), Slot::Item(b))).collect(),
        x: HashMap::new(),
        y: Vec::new(),
        folded: HashSet::new(),
        pitch: min_pitch.max(items.iter().map(|i| i.h).max().unwrap_or(55)).max(55),
        gap: VGAP,
        lane_ends: HashMap::new(),
    };

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
        if round == 2 || w as f64 <= h as f64 * MAX_WIDTH_RATIO {
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
    /// Everything here is decided by counts of crossings and by `(name, id)`
    /// on a tie, so the result cannot depend on how many sweeps happen to run
    /// or on the order links were pushed in.
    fn order(&mut self, items: &[Item]) {
        for row in self.rows.iter_mut() {
            row.sort_by_key(|s| slot_key(items, *s));
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

    /// Try every slot at every position in its row, keeping the best.
    ///
    /// A slot's crossings are counted against the rows above and below only,
    /// which is exact for a swap within one row. Repeated while it helps, to a
    /// fixed point.
    fn sift(&mut self, nb: &HashMap<Slot, Vec<Slot>>) {
        let depth = self.rows.len();
        let row_of: HashMap<Slot, usize> = self.row_index();

        for _round in 0..4 {
            let mut improved = false;
            for r in 0..depth {
                let n = self.rows[r].len();
                if n < 3 {
                    continue;
                }
                // Positions in the neighbouring rows, fixed for this row.
                let pos_other: HashMap<Slot, usize> = [r.wrapping_sub(1), r + 1]
                    .into_iter()
                    .filter(|&o| o < depth)
                    .flat_map(|o| self.rows[o].iter().enumerate().map(move |(i, s)| (*s, i)))
                    .collect();
                // Only slots that actually link to a neighbouring row can
                // change the count.
                let touched: Vec<usize> = (0..n)
                    .filter(|&i| {
                        nb.get(&self.rows[r][i])
                            .is_some_and(|ns| ns.iter().any(|o| pos_other.contains_key(o)))
                    })
                    .collect();
                if touched.len() < 2 {
                    continue;
                }

                // Crossings between two slots u (left) and v (right) of this
                // row, against both neighbouring rows.
                let cross_uv = |u: Slot, v: Slot| -> usize {
                    let mut c = 0;
                    for o in [r.wrapping_sub(1), r + 1] {
                        if o >= depth {
                            continue;
                        }
                        let us: Vec<usize> = nb
                            .get(&u)
                            .into_iter()
                            .flatten()
                            .filter(|s| row_of.get(s) == Some(&o))
                            .map(|s| pos_other[s])
                            .collect();
                        let vs: Vec<usize> = nb
                            .get(&v)
                            .into_iter()
                            .flatten()
                            .filter(|s| row_of.get(s) == Some(&o))
                            .map(|s| pos_other[s])
                            .collect();
                        for pu in &us {
                            for pv in &vs {
                                if pu > pv {
                                    c += 1;
                                }
                            }
                        }
                    }
                    c
                };

                let mut row = self.rows[r].clone();
                let touched: Vec<Slot> = touched.iter().map(|&i| row[i]).collect();
                for s in touched {
                    // Where it is *now* — earlier moves in this pass have
                    // shifted the indices.
                    let orig_i = row.iter().position(|t| *t == s).unwrap();
                    // Crossings this slot contributes at its current spot.
                    let score = |row: &[Slot], at: usize| -> usize {
                        let mut c = 0;
                        for (j, &t) in row.iter().enumerate() {
                            if j == at {
                                continue;
                            }
                            c += if j < at { cross_uv(t, s) } else { cross_uv(s, t) };
                        }
                        c
                    };
                    let mut best_at = orig_i;
                    let mut best_c = score(&row, orig_i);
                    // Try every other position. Strictly better only, and the
                    // earliest such position wins, so the result is fixed.
                    let without: Vec<Slot> = row.iter().copied().filter(|t| *t != s).collect();
                    for at in 0..=without.len() {
                        if at == orig_i {
                            continue;
                        }
                        let mut trial = without.clone();
                        trial.insert(at, s);
                        let c = score(&trial, at);
                        if c < best_c {
                            best_c = c;
                            best_at = at;
                        }
                    }
                    if best_at != orig_i {
                        let mut next = without;
                        next.insert(best_at, s);
                        row = next;
                        improved = true;
                    }
                }
                self.rows[r] = row;
            }
            if !improved {
                break;
            }
        }
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
    /// between its ends crosses the row: at the position that interpolates
    /// its two ends' positions in their rows, so that a vertical edge gets a
    /// vertical corridor and the boxes part around it. The move is kept only
    /// if the crossing count does not rise; the ordering's objective still
    /// comes first.
    fn straighten_corridors(&mut self) {
        let before = self.crossings();
        let saved = self.rows.clone();
        let row_of = self.row_index();
        // Ordinal position of every slot, as a fraction of its row, so rows
        // of different lengths interpolate sensibly.
        let frac = |rows: &Vec<Vec<Slot>>, s: Slot| -> f64 {
            let r = row_of[&s];
            let n = rows[r].len().max(1) as f64;
            let i = rows[r].iter().position(|t| *t == s).unwrap_or(0) as f64;
            (i + 0.5) / n
        };
        let mut wanted: HashMap<Slot, f64> = HashMap::new();
        for (ei, (upper, lower)) in &self.lane_ends {
            let (u, l) = (Slot::Item(*upper), Slot::Item(*lower));
            let (Some(&ru), Some(&rl)) = (row_of.get(&u), row_of.get(&l)) else { continue };
            let (fu, fl) = (frac(&self.rows, u), frac(&self.rows, l));
            let span = (rl - ru) as f64;
            for seg in 0..(rl - ru - 1) {
                let dm = Slot::Dummy(*ei, seg);
                if row_of.contains_key(&dm) {
                    let t = (seg as f64 + 1.0) / span;
                    wanted.insert(dm, fu + (fl - fu) * t);
                }
            }
        }
        if wanted.is_empty() {
            return;
        }
        for row in &mut self.rows {
            let n = row.len().max(1) as f64;
            let key = |s: &Slot, i: usize| -> f64 {
                match wanted.get(s) {
                    Some(f) => *f,
                    None => (i as f64 + 0.5) / n,
                }
            };
            let mut keyed: Vec<(f64, usize, Slot)> =
                row.iter().enumerate().map(|(i, s)| (key(s, i), i, *s)).collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
            *row = keyed.into_iter().map(|(_, _, s)| s).collect();
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

            let mut keyed: Vec<(f64, usize, Slot)> = self.rows[r]
                .iter()
                .enumerate()
                .map(|(i, &s)| {
                    let mut ps: Vec<usize> = nb
                        .get(&s)
                        .into_iter()
                        .flatten()
                        .filter_map(|o| pos.get(o).copied())
                        .collect();
                    ps.sort_unstable();
                    let median = if ps.is_empty() {
                        // Nothing to align with: hold position, so an
                        // unconnected node does not wander between runs.
                        i as f64
                    } else if ps.len() % 2 == 1 {
                        ps[ps.len() / 2] as f64
                    } else {
                        (ps[ps.len() / 2 - 1] + ps[ps.len() / 2]) as f64 / 2.0
                    };
                    (median, i, s)
                })
                .collect();

            // Ties keep their current order first, so an unconnected slot that
            // was held in place stays there relative to its neighbours, and
            // fall back to the name only when two slots are genuinely
            // interchangeable.
            keyed.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| slot_key(items, a.2).cmp(&slot_key(items, b.2)))
            });
            self.rows[r] = keyed.into_iter().map(|(_, _, s)| s).collect();
        }
    }

    /// Swap adjacent slots wherever that removes crossings, until it does not.
    ///
    /// Each swap is judged locally: only the crossings between the two slots
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
        let row_of: HashMap<Slot, usize> = self
            .rows
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.iter().map(move |s| (*s, r)))
            .collect();

        // Crossings that a pair of slots in one row contribute, given the
        // order they are in: `u` left of `v`. Counted against the row above
        // and the row below together.
        let count = |u: Slot, v: Slot, pos: &HashMap<Slot, usize>| -> usize {
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
        };

        let mut improved = true;
        let mut rounds = 0;
        while improved && rounds < 16 {
            improved = false;
            rounds += 1;
            for r in 0..depth {
                let mut i = 0;
                while i + 1 < self.rows[r].len() {
                    let (u, v) = (self.rows[r][i], self.rows[r][i + 1]);
                    let before = count(u, v, &pos);
                    let after = count(v, u, &pos);
                    // Strictly fewer, never equal: an equal swap would flip
                    // back on the next round and the pass would never settle.
                    if after < before {
                        self.rows[r].swap(i, i + 1);
                        pos.insert(u, i + 1);
                        pos.insert(v, i);
                        improved = true;
                    }
                    i += 1;
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
        if w as f64 <= h as f64 * MAX_WIDTH_RATIO {
            return;
        }
        let widest = self.rows.iter().map(|r| row_width(r, &self.width)).max().unwrap_or(0);
        let most = self.rows.iter().map(Vec::len).max().unwrap_or(1);
        for lines in 2..=most {
            let budget = (widest + lines as i32 - 1) / lines as i32;
            let (rows, folded) = fold_rows(&self.rows, budget, &self.width);
            let (w, h) = extent(&rows, &self.width, self.pitch);
            if w as f64 <= h as f64 * MAX_WIDTH_RATIO {
                self.rows = rows;
                self.folded = folded;
                return;
            }
        }
    }

    /// Place each slot near the median of what it connects to, then push apart.
    ///
    /// Aligning a node with its neighbours is what makes an edge vertical
    /// instead of diagonal, and a vertical edge is one that crosses nothing.
    ///
    /// Slots are placed by **priority**: within a row, the slot with the most
    /// links to the row it is being aligned against goes first and takes its
    /// ideal position; the rest are fitted around what is already down. Placing
    /// left to right instead — every slot yielding to whatever sits on its left
    /// — lets one leaf that happens to come first push a hub with ten children
    /// off its column, and the ten edges then run diagonally across the row.
    /// On a tree that came out as a right-leaning staircase, with every row
    /// anchored at the same left edge and holes of several hundred pixels
    /// where the medians wanted to be.
    ///
    /// The lines of a folded rank are placed as one block. They have no links
    /// to each other, so placed one line at a time each would take its own
    /// median from the rank below and pack rightward from that single spot,
    /// and the rank below then recentre on where they landed — a little to the
    /// right — on every sweep, until the fan walked off the page. As a block
    /// they take one median and stay stacked.
    fn assign_x(&mut self, items: &[Item], edges: &[(usize, usize)]) {
        let nb = self.neighbours();
        let depth = self.rows.len();

        // A first pass left to right gives every slot a position.
        for row in &self.rows {
            let mut cursor = 0;
            for s in row {
                self.x.insert(*s, cursor);
                cursor += self.width[s] + HGAP;
            }
        }

        // The rows that will be swept as units: a folded rank's lines together,
        // every other row alone.
        let mut units: Vec<Vec<usize>> = Vec::new();
        let mut r = 0;
        while r < depth {
            if self.folded.contains(&r) {
                let start = r;
                while r < depth && self.folded.contains(&r) {
                    r += 1;
                }
                units.push((start..r).collect());
            } else {
                units.push(vec![r]);
                r += 1;
            }
        }

        let sweep = |g: &mut Self, rounds: usize| {
            for sweep in 0..rounds {
                let down = sweep % 2 == 0;
                let seq: Vec<usize> = if down {
                    (1..units.len()).collect()
                } else {
                    (0..units.len().saturating_sub(1)).rev().collect()
                };
                for ui in seq {
                    let unit = units[ui].clone();
                    let other: Vec<usize> =
                        if down { units[ui - 1].clone() } else { units[ui + 1].clone() };
                    let other_set: HashSet<usize> = other.iter().copied().collect();
                    if unit.len() == 1 {
                        g.place_row(unit[0], &other_set, &nb);
                    } else {
                        g.place_block(&unit, &other_set, &nb);
                    }
                }
            }
        };
        sweep(self, 6);
        let _ = items;

        // Close the gaps that separate loosely joined groups. Each row is
        // placed against its neighbours' medians, and a group whose members
        // link mostly to each other is self-consistent wherever it sits — so
        // two such groups joined by a few edges settle far apart with those
        // edges stretched right across the drawing. Nothing in the sweep
        // pulls a *group* toward another. This does: every gap in every row
        // is tried closed, and stays closed if the drawing's edges get
        // shorter in total. Closing a gap in one row moves the medians of the
        // rows beside it, so a short sweep follows and the two alternate.
        for _ in 0..3 {
            self.close_gaps(&nb);
            sweep(self, 2);
        }
        self.close_gaps(&nb);

        // Edges are straight lines, and a corridor only protects its edge
        // if it is as wide as the line's slant across the row. Now that the
        // boxes have settled: pick the row gap that lets every adjacent edge
        // clear its rows, size each corridor to its line's sweep at that gap,
        // and let the boxes fit around it. The ends move a little for that,
        // and the gap depends on where they are, so the two go round
        // together, and the gap is fixed last against the boxes as drawn.
        for _ in 0..3 {
            self.fit_gap(edges);
            self.size_lanes();
            sweep(self, 2);
            self.close_gaps(&nb);
        }
        self.fit_gap(edges);

        // Shift so the leftmost edge of the drawing sits at zero.
        let min = self.x.values().copied().min().unwrap_or(0);
        for v in self.x.values_mut() {
            *v = snap(*v - min);
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
        let step = (self.pitch + self.gap) as f64;
        let mut widths: Vec<(Slot, i32)> = Vec::new();
        for (ei, (upper, lower)) in &self.lane_ends {
            let (u, l) = (Slot::Item(*upper), Slot::Item(*lower));
            let (Some(&ru), Some(&rl)) = (row_of.get(&u), row_of.get(&l)) else { continue };
            let ux = (self.x[&u] + self.width[&u] / 2) as f64;
            let lx = (self.x[&l] + self.width[&l] / 2) as f64;
            let per_row = (lx - ux) / (rl - ru) as f64;
            let sweep = (per_row.abs() * self.pitch as f64 / step).ceil() as i32;
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

    /// Grow the row gap until no straight edge between adjacent rows cuts
    /// through a box in either of its own rows.
    ///
    /// A line from one row to the next leaves its box through the bottom if
    /// it is steep and through the side if it is shallow, and a shallow one
    /// then runs along the row band through the neighbours before it drops.
    /// How shallow is shallow depends on the gap: the further apart the rows,
    /// the steeper every line. So the gap is the one number that makes every
    /// adjacent edge in the drawing clear its rows — computed exactly, not
    /// tuned — and capped, so one edge that spans the width of the drawing
    /// costs a taller drawing but not an absurd one.
    fn fit_gap(&mut self, edges: &[(usize, usize)]) {
        let row_of = self.row_index();
        let mut need = VGAP;
        for &(a, b) in edges {
            if a == b {
                continue;
            }
            let (sa, sb) = (Slot::Item(a), Slot::Item(b));
            let (Some(&ra), Some(&rb)) = (row_of.get(&sa), row_of.get(&sb)) else { continue };
            if ra.abs_diff(rb) != 1 {
                continue;
            }
            let (up, dn) = if ra < rb { (sa, sb) } else { (sb, sa) };
            let (ux, dx) = (self.x[&up] + self.width[&up] / 2, self.x[&dn] + self.width[&dn] / 2);
            let (upper_row, lower_row) = (row_of[&up], row_of[&dn]);
            // Every other box in the two rows that lies between the ends
            // horizontally: the line must be past the row band by the time
            // it reaches the box's near edge.
            for (row, from_top) in [(upper_row, true), (lower_row, false)] {
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
                    // this far by `run_to_box`. Its total drop is pitch+gap
                    // over `run`; solve for the gap.
                    let half = (self.height[o].max(1) as f64) / 2.0 + 2.0;
                    let g = (half * run / run_to_box.max(1.0) - self.pitch as f64).ceil() as i32;
                    need = need.max(g);
                }
            }
        }
        self.gap = snap(need.min(VGAP * 4)).max(VGAP);
    }

    /// Slide everything to the right of a vertical cut leftward, wherever that
    /// shortens the drawing's edges overall.
    ///
    /// Two groups of boxes that link mostly to themselves are each
    /// self-consistent wherever they sit, and the sweep leaves them wherever
    /// the ordering happened to put them — often at opposite ends of the
    /// drawing, with the few edges between them stretched right across it.
    /// Closing a gap one row at a time cannot fix that: the row that moves
    /// leaves the rows above and below behind, and its own edges to them get
    /// longer than the cross-group edges get shorter. So the move here is
    /// across every row at once — everything to the right of a vertical cut
    /// slides left together, by as much as the tightest row allows — and only
    /// the edges that cross the cut change length. Kept if they get shorter.
    ///
    /// The cuts tried are the right edge of every slot: the places a gap can
    /// start. Rows in order, cuts left to right, strictly better only, and
    /// repeated to a fixed point, so the result is determined.
    fn close_gaps(&mut self, nb: &HashMap<Slot, Vec<Slot>>) {
        let all: Vec<Slot> = self.rows.iter().flatten().copied().collect();
        for _round in 0..8 {
            let mut any = false;
            let mut cuts: Vec<i32> = all.iter().map(|s| self.x[s] + self.width[s]).collect();
            cuts.sort_unstable();
            cuts.dedup();
            for cut in cuts {
                // How far can everything at or beyond the cut move left? The
                // least clearance any row has at the cut.
                let mut room = i32::MAX;
                for row in &self.rows {
                    let mut left_edge = i32::MIN;
                    let mut right_start = i32::MAX;
                    for s in row {
                        let (x0, x1) = (self.x[s], self.x[s] + self.width[s]);
                        if x0 >= cut {
                            right_start = right_start.min(x0);
                        } else {
                            left_edge = left_edge.max(x1);
                        }
                    }
                    if right_start != i32::MAX && left_edge != i32::MIN {
                        room = room.min(right_start - left_edge - HGAP);
                    }
                }
                if room == i32::MAX || room <= 0 {
                    continue;
                }
                let moved: HashSet<Slot> =
                    all.iter().copied().filter(|s| self.x[s] >= cut).collect();
                if moved.is_empty() || moved.len() == all.len() {
                    continue;
                }
                // Only edges crossing the cut change. Sum |dx| before and after.
                let mut before: i64 = 0;
                let mut after: i64 = 0;
                for m in &moved {
                    let mx = self.x[m] + self.width[m] / 2;
                    for o in nb.get(m).into_iter().flatten() {
                        if moved.contains(o) {
                            continue;
                        }
                        let ox = self.x[o] + self.width[o] / 2;
                        before += (mx - ox).abs() as i64;
                        after += (mx - room - ox).abs() as i64;
                    }
                }
                if after < before {
                    for m in &moved {
                        *self.x.get_mut(m).unwrap() -= room;
                    }
                    any = true;
                }
            }
            if !any {
                break;
            }
        }
    }

    /// Where a slot wants to be, given the rows it is aligned against: the
    /// median centre of its links there, and how many such links it has.
    fn wish(
        &self,
        s: Slot,
        other: &HashSet<usize>,
        row_of: &HashMap<Slot, usize>,
        nb: &HashMap<Slot, Vec<Slot>>,
    ) -> Option<(i32, usize)> {
        // A corridor neighbour counts for more than a box neighbour. Edges are
        // straight lines, and a long edge only stays inside its corridor if
        // its two ends sit over the corridor's column; a box that is the end
        // of a long edge and also has several short ones would otherwise be
        // pulled to the median of the short ones and the long line would
        // slant across the ranks it skips. Three votes for a corridor, one
        // for a box, and the median of that.
        let mut centres: Vec<i32> = Vec::new();
        for o in nb.get(&s).into_iter().flatten() {
            if !row_of.get(o).is_some_and(|r| other.contains(r)) {
                continue;
            }
            let c = self.x[o] + self.width[o] / 2;
            let votes = if matches!(o, Slot::Dummy(..)) { LANE_VOTES } else { 1 };
            centres.extend(std::iter::repeat_n(c, votes));
        }
        if centres.is_empty() {
            return None;
        }
        centres.sort_unstable();
        let n = centres.len();
        let median =
            if n % 2 == 1 { centres[n / 2] } else { (centres[n / 2 - 1] + centres[n / 2]) / 2 };
        Some((median - self.width[&s] / 2, n))
    }

    fn row_index(&self) -> HashMap<Slot, usize> {
        self.rows.iter().enumerate().flat_map(|(r, row)| row.iter().map(move |s| (*s, r))).collect()
    }

    /// Place one row against its neighbouring row(s), by priority.
    fn place_row(&mut self, r: usize, other: &HashSet<usize>, nb: &HashMap<Slot, Vec<Slot>>) {
        let row_of = self.row_index();
        let order = self.rows[r].clone();
        let n = order.len();
        if n == 0 {
            return;
        }

        // Each slot's target and priority. A slot with no links to the other
        // side keeps where it is, at the lowest priority.
        //
        // A dummy — one row of a long edge's corridor — is placed before any
        // real box. It has exactly one link each way, so by link count it would
        // go last and take whatever room was left, and the corridor would end
        // up somewhere other than under the edge's ends. The edge then needs a
        // bend to reach it, on a drawing that could have been straight. Placing
        // the corridor first keeps the edge straight and lets the boxes, which
        // can sit anywhere in their row without their edges getting longer,
        // fit around it. This is the priority order in Gansner et al.
        let plan: Vec<(i32, usize)> = order
            .iter()
            .map(|s| {
                let (t, links) = self.wish(*s, other, &row_of, nb).unwrap_or((self.x[s], 0));
                let priority = if matches!(s, Slot::Dummy(..)) { usize::MAX } else { links };
                (t, priority)
            })
            .collect();

        // Priority order: dummies, then most links first; ties by current
        // position so a sweep is deterministic and stable.
        let mut by_priority: Vec<usize> = (0..n).collect();
        by_priority.sort_by(|&a, &b| plan[b].1.cmp(&plan[a].1).then_with(|| a.cmp(&b)));

        // Positions in row order, filled in as slots are placed. A slot placed
        // earlier is fixed and later ones must fit between the fixed ones.
        let mut placed: Vec<Option<i32>> = vec![None; n];
        for &i in &by_priority {
            let (target, _) = plan[i];
            let w = self.width[&order[i]];
            // The nearest fixed slot on either side bounds where this one can
            // go: everything unfixed between them will be squeezed in later, so
            // room for those has to be kept too.
            let mut lo = i32::MIN;
            let mut need_left = 0;
            for j in (0..i).rev() {
                match placed[j] {
                    Some(x) => {
                        lo = x + self.width[&order[j]] + HGAP + need_left;
                        break;
                    }
                    None => need_left += self.width[&order[j]] + HGAP,
                }
            }
            let mut hi = i32::MAX;
            let mut need_right = 0;
            for j in (i + 1)..n {
                match placed[j] {
                    Some(x) => {
                        hi = x - HGAP - need_right - w;
                        break;
                    }
                    None => need_right += self.width[&order[j]] + HGAP,
                }
            }
            let x = if lo != i32::MIN && hi != i32::MAX && lo > hi {
                // Cannot fit between the fixed neighbours at all — this can
                // only happen if earlier placements were too tight, which the
                // reservation above prevents; but be safe and lean left.
                lo
            } else {
                target.clamp(lo.min(hi), hi.max(lo))
            };
            placed[i] = Some(x);
        }
        for (i, s) in order.iter().enumerate() {
            self.x.insert(*s, placed[i].unwrap());
        }
    }

    /// Place the lines of a folded rank as one block against a neighbouring
    /// row: keep each line packed as it is, and shift them all together so the
    /// block's centre sits on the median of everything the block links to.
    fn place_block(
        &mut self,
        lines: &[usize],
        other: &HashSet<usize>,
        nb: &HashMap<Slot, Vec<Slot>>,
    ) {
        let row_of = self.row_index();
        // Pack each line tight, so the block is compact whatever the first
        // pass left.
        for &r in lines {
            let mut cursor = 0;
            for s in &self.rows[r].clone() {
                self.x.insert(*s, cursor);
                cursor += self.width[s] + HGAP;
            }
        }
        // Everything the block links to on the other side.
        let mut centres: Vec<i32> = Vec::new();
        for &r in lines {
            for s in &self.rows[r] {
                for o in nb.get(s).into_iter().flatten() {
                    if row_of.get(o).is_some_and(|rr| other.contains(rr)) {
                        centres.push(self.x[o] + self.width[o] / 2);
                    }
                }
            }
        }
        if centres.is_empty() {
            return;
        }
        centres.sort_unstable();
        let target_centre = centres[centres.len() / 2];
        let block_w =
            lines.iter().map(|&r| row_width(&self.rows[r], &self.width)).max().unwrap_or(0);
        let shift = target_centre - block_w / 2;
        for &r in lines {
            // Centre each line inside the block width, then shift the block.
            let lw = row_width(&self.rows[r], &self.width);
            let inset = (block_w - lw) / 2;
            for s in &self.rows[r].clone() {
                let x = self.x[s] + inset + shift;
                self.x.insert(*s, x);
            }
        }
    }

    /// The height every row is given: the tallest box in the drawing.
    ///
    /// One pitch for all rows rather than each row's own tallest, so that a
    /// drawing reads as a grid — and so that when components are laid out
    /// separately and packed side by side, their rows line up across the
    /// shelf instead of drifting by the height of one taller box.
    fn row_pitch(&self) -> i32 {
        self.pitch
    }

    fn assign_y(&mut self) {
        let h = self.row_pitch();
        let g = self.gap;
        self.y = (0..self.rows.len()).map(|r| snap(r as i32 * (h + g))).collect();
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
        assert!(adjacent > 3000 && long > 1000, "the sweep proves little: {adjacent} / {long}");
        let share = long_through as f64 / long as f64;
        eprintln!("long edges through a box: {long_through} of {long} ({share:.3})");
        assert!(share < 0.30, "{long_through} of {long} long edges through a box ({share:.2})");
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
