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
//! **Lanes, not bends.** An edge crossing several rows reserves a corridor in
//! each one, which keeps other boxes out of its way.
//!
//! **Bends only where a straight line would actually hit something.** The
//! corridor usually leaves the direct line clear, and then the edge is drawn
//! straight. Adding a bendpoint because an edge *might* need one is how a
//! diagram ends up full of kinks that buy nothing.
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

/// Where everything goes, and how the long edges get there.
#[derive(Clone, Debug, Default)]
pub struct Placement {
    /// One rectangle per item, in the order given.
    pub rects: Vec<Rect>,
    /// Absolute waypoints for an edge, by its index in the input. Only edges
    /// that need routing appear here.
    pub routes: HashMap<usize, Vec<Pt>>,
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
            // Folding wide rows normally keeps layering inside the bound on its
            // own, so reaching here means a row could not be folded — it was
            // holding a corridor for an edge passing through.
            //
            // The grid is squarer, but it places by name and ignores every
            // edge, so on a connected graph its lines cross far more: on a set
            // of real views, three to ten times as often. A drawing five times
            // wider than tall with a hundred crossings reads; a square one
            // with a thousand does not. So the fallback has to win on both
            // counts — squarer, and not paying for it in tangles — or the
            // layering stands. It still catches what it was made for, the
            // shallow graph with almost no edges to tangle, where the grid is
            // as good and much narrower.
            let squared = grid_placement(items);
            let squarer = wideness(&squared.rects) < ratio;
            let (lc, gc) = (drawn_crossings(&layered, edges), drawn_crossings(&squared, edges));
            let no_worse = gc <= lc.max(1) * 3 / 2;
            if squarer && no_worse { squared } else { layered }
        }
    }
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
    Placement { rects: grid(items), routes: HashMap::new(), algorithm: Algorithm::Grid }
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
fn extent(
    rows: &[Vec<Slot>],
    width: &HashMap<Slot, i32>,
    height: &HashMap<Slot, i32>,
) -> (i32, i32) {
    let w = rows.iter().map(|r| row_width(r, width)).max().unwrap_or(0);
    let mut h = 0;
    for (i, row) in rows.iter().enumerate() {
        h += row.iter().map(|s| height[s]).max().unwrap_or(55).max(55);
        if i + 1 < rows.len() {
            h += VGAP;
        }
    }
    (w, h)
}

/// Break rows wider than `budget` into lines, leaving the rest untouched.
///
/// Returns the new rows, and the indices of those that are lines of a folded
/// rank rather than ranks in their own right.
///
/// A row holding a dummy is never broken. That dummy is one row of a corridor
/// reserved for an edge passing through, and [`Layered::routes`] reads the
/// corridor off one row per rank; splitting the row would leave the edge with
/// two lanes at the same rank and nothing to say which it runs down.
fn fold_rows(
    rows: &[Vec<Slot>],
    budget: i32,
    width: &HashMap<Slot, i32>,
) -> (Vec<Vec<Slot>>, HashSet<usize>) {
    let mut out: Vec<Vec<Slot>> = Vec::with_capacity(rows.len());
    let mut folded: HashSet<usize> = HashSet::new();
    for row in rows {
        let holds_a_corridor = row.iter().any(|s| matches!(s, Slot::Dummy(..)));
        if row.len() < 2 || holds_a_corridor || row_width(row, width) <= budget {
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

fn sugiyama(items: &[Item], edges: &[(usize, usize)]) -> Placement {
    let (raw_rank, reversed) = rank_by_dependency(items.len(), edges);
    let ranks = compact(&raw_rank);
    let g = build(items, edges, &ranks, &reversed.iter().copied().collect::<Vec<_>>());
    let rects = g.finish(items);
    let routes = g.routes(items, edges, &rects);
    Placement { rects, routes, algorithm: Algorithm::Sugiyama }
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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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
}

fn build(items: &[Item], edges: &[(usize, usize)], ranks: &[usize], reversed: &[usize]) -> Layered {
    let depth = ranks.iter().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<Slot>> = vec![Vec::new(); depth];
    let mut width = HashMap::new();
    let mut height = HashMap::new();

    for (i, item) in items.iter().enumerate() {
        rows[ranks[i]].push(Slot::Item(i));
        width.insert(Slot::Item(i), item.w);
        height.insert(Slot::Item(i), item.h);
    }

    // A long edge becomes a chain of dummies, one per row it crosses. Those
    // dummies take up space in the ordering, which is exactly what keeps the
    // edge from being drawn over the top of a box.
    let mut links: Vec<(Slot, Slot)> = Vec::new();
    for (ei, (a, b)) in edges.iter().enumerate() {
        let (a, b) = if reversed.contains(&ei) { (*b, *a) } else { (*a, *b) };
        if a == b {
            continue;
        }
        let (r0, r1) = (ranks[a], ranks[b]);
        if r0 == r1 {
            continue; // handled at routing time
        }
        let (lo, hi, from, to) = if r0 < r1 {
            (r0, r1, Slot::Item(a), Slot::Item(b))
        } else {
            (r1, r0, Slot::Item(b), Slot::Item(a))
        };

        let mut prev = from;
        for (seg, row) in ((lo + 1)..hi).enumerate() {
            let d = Slot::Dummy(ei, seg);
            rows[row].push(d);
            width.insert(d, DUMMY_W);
            height.insert(d, 0);
            links.push((prev, d));
            prev = d;
        }
        links.push((prev, to));
    }

    let mut g = Layered {
        rows,
        width,
        height,
        links,
        x: HashMap::new(),
        y: Vec::new(),
        folded: HashSet::new(),
    };
    g.order(items);
    // After ordering, so a fold inherits the sequence that crossed least, and
    // before placement, so the folded lines are packed like any other row.
    g.fold_to_fit();
    g.assign_x(items);
    g.assign_y();
    g
}

impl Layered {
    /// Order within each row to reduce crossings between adjacent rows.
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
    /// If no fold fits, the rows are left as they were. Only a row pinned by a
    /// corridor can get there, and an unfoldable row is exactly what `auto`'s
    /// fallback to a grid is still holding for.
    fn fold_to_fit(&mut self) {
        let (w, h) = extent(&self.rows, &self.width, &self.height);
        if w as f64 <= h as f64 * MAX_WIDTH_RATIO {
            return;
        }
        let widest = self.rows.iter().map(|r| row_width(r, &self.width)).max().unwrap_or(0);
        let most = self.rows.iter().map(Vec::len).max().unwrap_or(1);
        for lines in 2..=most {
            let budget = (widest + lines as i32 - 1) / lines as i32;
            let (rows, folded) = fold_rows(&self.rows, budget, &self.width);
            let (w, h) = extent(&rows, &self.width, &self.height);
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
    fn assign_x(&mut self, items: &[Item]) {
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

        for sweep in 0..6 {
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
                    self.place_row(unit[0], &other_set, &nb);
                } else {
                    self.place_block(&unit, &other_set, &nb);
                }
            }
        }
        let _ = items;

        // Shift so the leftmost edge of the drawing sits at zero.
        let min = self.x.values().copied().min().unwrap_or(0);
        for v in self.x.values_mut() {
            *v = snap(*v - min);
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
        let mut centres: Vec<i32> = nb
            .get(&s)
            .into_iter()
            .flatten()
            .filter(|o| row_of.get(o).is_some_and(|r| other.contains(r)))
            .map(|o| self.x[o] + self.width[o] / 2)
            .collect();
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

    fn assign_y(&mut self) {
        self.y = Vec::with_capacity(self.rows.len());
        let mut cursor = 0;
        for row in &self.rows {
            self.y.push(snap(cursor));
            let h = row.iter().map(|s| self.height[s]).max().unwrap_or(55).max(55);
            cursor += h + VGAP;
        }
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

    /// Waypoints, for the edges that genuinely need them.
    ///
    /// The corridor a long edge reserved usually leaves the straight line
    /// clear, and a straight line is better than a routed one every time. So
    /// the direct segment is tested against every box first, and only an edge
    /// that would actually cut through something gets bendpoints. Adding a bend
    /// because an edge *might* need one is how a diagram fills up with kinks
    /// that buy nothing.
    fn routes(
        &self,
        items: &[Item],
        edges: &[(usize, usize)],
        rects: &[Rect],
    ) -> HashMap<usize, Vec<Pt>> {
        // Where each long edge's corridor runs, kept aside until we know
        // whether it is needed.
        //
        // A corridor is entered above the row it crosses and left below it,
        // rather than being marked with a single point at the row's middle.
        // One point leaves the edge approaching it diagonally from the source,
        // and that diagonal is drawn across the row it was supposed to bypass —
        // on a crowded rank it clips the corner of whatever it passes, so the
        // lane stays clear and the edge goes through a box anyway. Bracketing
        // the row keeps the run *through* the rank vertical and inside the
        // lane, and leaves the diagonals in the gaps between rows, where by
        // construction there is nothing to hit.
        // Row heights, counted as `assign_y` counted them, and which row each
        // item landed in — a corridor is bracketed against its row's band
        // rather than against any one box in it, so a row holding a tall box
        // brackets every lane in it the same way.
        let row_h: Vec<i32> = self
            .rows
            .iter()
            .map(|row| row.iter().map(|s| self.height[s]).max().unwrap_or(55).max(55))
            .collect();
        let mut row_of: HashMap<usize, usize> = HashMap::new();
        for (r, row) in self.rows.iter().enumerate() {
            for s in row {
                if let Slot::Item(i) = s {
                    row_of.insert(*i, r);
                }
            }
        }

        let mut lanes: HashMap<usize, Vec<(usize, Pt, Pt)>> = HashMap::new();
        for (r, row) in self.rows.iter().enumerate() {
            let h = row_h[r];
            for s in row {
                let Slot::Dummy(ei, seg) = s else { continue };
                let x = self.x[s] + DUMMY_W / 2;
                let enter = Pt { x, y: self.y[r] - VGAP / 2 };
                let leave = Pt { x, y: self.y[r] + h + VGAP / 2 };
                lanes.entry(*ei).or_default().push((*seg, enter, leave));
            }
        }

        let mut out: HashMap<usize, Vec<Pt>> = HashMap::new();
        for (ei, (a, b)) in edges.iter().enumerate() {
            if a == b {
                continue;
            }
            let (ra, rb) = (rects[*a], rects[*b]);

            // Same row: a straight line would run along the row, through
            // anything between the two. Bow it below unless they are adjacent.
            if ra.y == rb.y {
                let (left, right) = if ra.x <= rb.x { (*a, *b) } else { (*b, *a) };
                let gap = rects[right].x - (rects[left].x + rects[left].w);
                if gap <= HGAP + 8 {
                    continue;
                }
                let y = rects[left].y + rects[left].h + 20 + (ei as i32 % 3) * 10;
                let mut pts = vec![
                    Pt { x: rects[left].x + rects[left].w / 2, y },
                    Pt { x: rects[right].x + rects[right].w / 2, y },
                ];
                if ra.x > rb.x {
                    pts.reverse();
                }
                out.insert(ei, pts);
                continue;
            }

            if straight_is_clear(ra, rb, rects, *a, *b) {
                continue;
            }

            let Some(lane) = lanes.get(&ei) else { continue };
            let mut lane: Vec<(usize, Pt, Pt)> = lane.clone();
            lane.sort_by_key(|(seg, _, _)| *seg);

            // Leave the upper box straight down and come into the lower one
            // straight from above, so that both ends clear their own rank the
            // way the corridor clears the ranks between. Coming out of a centre
            // at whatever angle the first corridor happens to sit at is the
            // same mistake as the single mid-row waypoint, moved to the ends:
            // the diagonal crosses the row the box itself is in and clips its
            // neighbours.
            let (upper, lower) = if ra.y <= rb.y { (*a, *b) } else { (*b, *a) };
            let (ur, lr) = (row_of[&upper], row_of[&lower]);
            let mut pts = vec![Pt {
                x: rects[upper].x + rects[upper].w / 2,
                y: self.y[ur] + row_h[ur] + VGAP / 2,
            }];
            // Where one row's corridor runs down the same column as the next,
            // leaving that row and entering the following one are the same
            // point, and the edge should carry one bend there rather than two.
            for (_, enter, leave) in lane {
                if pts.last() != Some(&enter) {
                    pts.push(enter);
                }
                pts.push(leave);
            }
            pts.push(Pt { x: rects[lower].x + rects[lower].w / 2, y: self.y[lr] - VGAP / 2 });

            // Every leg of that route is either vertical inside a reserved
            // column or horizontal along a gap between rows, so it is clear —
            // but it is also more bends than most edges need. Drop each one
            // that the drawing does not actually depend on, earliest first, so
            // an edge keeps only the kinks that are holding it off a box.
            //
            // The trial path runs upper to lower, the way `pts` was built.
            // Threading it from `a` to `b` instead is only the same path when
            // the edge points down; for one pointing up it tests the mirror
            // image, keeps a bend the real path does not need and drops one it
            // does, and the drawn edge cuts a box the trial never saw.
            let ends = (rects[upper].center(), rects[lower].center());
            let mut i = 0;
            while i < pts.len() {
                let trial: Vec<Pt> = std::iter::once(ends.0)
                    .chain(pts.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p))
                    .chain(std::iter::once(ends.1))
                    .collect();
                if path_is_clear(&trial, rects, *a, *b) {
                    pts.remove(i);
                } else {
                    i += 1;
                }
            }

            // The chain was built from the upper end downwards; an edge drawn
            // upward needs it the other way round.
            if ra.y > rb.y {
                pts.reverse();
            }
            if !pts.is_empty() {
                out.insert(ei, pts);
            }
        }

        let _ = items;
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

    /// The whole polyline an edge is drawn as: one centre, any waypoints, the
    /// other centre.
    fn drawn_path(p: &Placement, edges: &[(usize, usize)], ei: usize) -> Vec<Pt> {
        let (a, b) = edges[ei];
        let mut path = vec![p.rects[a].center()];
        path.extend(p.routes.get(&ei).into_iter().flatten().copied());
        path.push(p.rects[b].center());
        path
    }

    /// A long edge skips a rank, so by construction something shares that rank.
    /// Whether the corridor clears the line by moving the box aside or the edge
    /// bends around it is the layout's business; what it may never do is cross
    /// it.
    #[test]
    fn a_long_edge_never_crosses_the_rank_it_skips() {
        // 0 -> 1 -> 2 with an extra 0 -> 2 that skips a rank. The lane pushes
        // box 1 aside, so the edge comes out straight — and a straight line
        // that misses beats a bend that also misses.
        let it = items(3);
        let edges = vec![(0, 1), (1, 2), (0, 2)];
        let p = place(&it, &edges, Algorithm::Sugiyama);
        let path = drawn_path(&p, &edges, 2);
        assert!(
            !path.windows(2).any(|s| segment_hits(s[0], s[1], p.rects[1])),
            "the skipping edge crosses box 1: {path:?} vs {:?}",
            p.rects[1]
        );

        // Crowd the middle rank and there is nowhere to move aside to, so the
        // edge has to bend instead — and the bends have to miss as well.
        let it = items(8);
        let mut edges = vec![(0, 1), (1, 7), (0, 7)];
        for i in 2..7 {
            edges.push((0, i));
            edges.push((i, 7));
        }
        let p = place(&it, &edges, Algorithm::Sugiyama);
        assert!(!p.routes[&2].is_empty(), "a crowded rank leaves the long edge no straight line");
        let path = drawn_path(&p, &edges, 2);
        for (k, other) in p.rects.iter().enumerate() {
            if k == 0 || k == 7 {
                continue;
            }
            assert!(
                !path.windows(2).any(|s| segment_hits(s[0], s[1], *other)),
                "the routed edge crosses box {k}: {path:?} vs {other:?}"
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
        assert!(
            wideness(&layered.rects) <= MAX_WIDTH_RATIO,
            "the fold should have brought it inside the bound, got {:?}",
            wideness(&layered.rects)
        );

        // The fold must not cost the ranking: every source still sits above
        // every target, which is the whole reason not to fall back to a grid.
        let lowest_source = (0..30).map(|i| layered.rects[i].y).max().unwrap();
        let highest_target = (30..60).map(|i| layered.rects[i].y).min().unwrap();
        assert!(lowest_source < highest_target, "the fold broke the ranking");

        // And it beats the grid it used to fall back to.
        let squared = place(&it, &edges, Algorithm::Grid);
        assert!(wideness(&layered.rects) < wideness(&squared.rects) * 2.0);

        // `auto` now keeps the layering, because there is nothing left to
        // rescue it from.
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

    /// The invariant behind every routing decision, over a spread of shapes
    /// rather than one hand-picked graph.
    ///
    /// A single case is easy to satisfy by accident — the first attempt at this
    /// bracketed the crossed rank and fixed the reported drawing while still
    /// cutting a corner off a box on a hundred and thirty-six others, because
    /// the legs into the source and out of the target cross their own ranks
    /// too. The sweep is what caught that.
    #[test]
    fn no_routed_edge_is_drawn_through_a_box() {
        // A fixed sequence, so a failure here is reproducible rather than
        // something that shows up one run in ten.
        let mut seed = 12345u64;
        let mut rnd = |m: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % m.max(1)
        };

        let mut routed = 0;
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
            for (ei, (a, b)) in edges.iter().enumerate() {
                if !p.routes.contains_key(&ei) {
                    continue;
                }
                routed += 1;
                let path = drawn_path(&p, &edges, ei);
                assert!(
                    path_is_clear(&path, &p.rects, *a, *b),
                    "trial {trial}: edge {ei} ({a} -> {b}) is drawn through a box: {path:?}"
                );
            }
        }
        assert!(routed > 500, "only {routed} edges needed routing; the sweep proves little");
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
