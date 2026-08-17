//! Placing concepts on a new view.
//!
//! Rows come from the dependency graph and from nothing else. The ArchiMate
//! layer is deliberately not consulted: most relationships in a real model run
//! *within* a layer, so ranking by layer puts them all in one row and turns each
//! into a horizontal line slicing through whatever sits between its ends.
//!
//! Four things do the work.
//!
//! **Tight ranking.** Longest-path layering alone shoves every node as high as
//! it can go, which stretches edges for no reason — a node whose only successor
//! is four rows down gets dragged to the top. Each node is then pulled back down
//! to sit directly above its earliest successor, so most edges span exactly one
//! row.
//!
//! **Folding a rank that will not fit.** A hundred motivation elements two
//! ranks deep give layering nothing to stack, and the rank runs off the side of
//! any screen. Such a rank is folded onto several lines, which keeps the
//! ranking that a fallback to a grid would throw away.
//!
//! **Lanes, not bends.** An edge crossing several rows reserves a corridor in
//! each one, which keeps other boxes out of its way.
//!
//! **Bends only where a straight line would actually hit something.** The
//! corridor usually leaves the direct line clear, and then the edge is drawn
//! straight. Adding a bendpoint because an edge *might* need one is how a
//! diagram ends up full of kinks that buy nothing.
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
            // Only swap if the grid is actually narrower: on a graph that is
            // genuinely one long row of siblings it is not, and falling back
            // would lose the layering for nothing.
            let squared = grid_placement(items);
            if wideness(&squared.rects) < ratio { squared } else { layered }
        }
    }
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

/// Longest-path ranking over the dependency graph, with cycles broken first.
///
/// The rank of a node is one more than the deepest of its predecessors, so
/// every edge points downward and, before dummies, spans at least one row.
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

    // Longest-path alone puts every node as high as it can go, which stretches
    // edges for nothing: a node whose only successor is four rows down gets
    // dragged to the top and its edge then has to cross three rows. Pull each
    // node back down until it sits directly above its earliest successor.
    // Sinks stay put, so the drawing does not collapse.
    loop {
        let mut moved = false;
        for v in 0..n {
            if succ[v].is_empty() {
                continue;
            }
            let latest = succ[v].iter().map(|w| rank[*w]).min().unwrap_or(rank[v] + 1) - 1;
            if latest > rank[v] {
                rank[v] = latest;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    (rank, reversed)
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
    fn order(&mut self, items: &[Item]) {
        for row in self.rows.iter_mut() {
            row.sort_by_key(|s| slot_key(items, *s));
        }
        let mut best = self.rows.clone();
        let mut best_score = self.crossings();

        for sweep in 0..8 {
            self.median_pass(items, sweep % 2 == 0);
            let score = self.crossings();
            // Strictly better only, so the earliest sweep wins a tie and the
            // result cannot depend on how many sweeps happen to run.
            if score < best_score {
                best_score = score;
                best = self.rows.clone();
            }
        }
        self.rows = best;
    }

    fn median_pass(&mut self, items: &[Item], down: bool) {
        let sequence: Vec<usize> = if down {
            (1..self.rows.len()).collect()
        } else {
            (0..self.rows.len().saturating_sub(1)).rev().collect()
        };

        for r in sequence {
            let neighbour_row = if down { r - 1 } else { r + 1 };
            let pos: HashMap<Slot, usize> =
                self.rows[neighbour_row].iter().enumerate().map(|(i, s)| (*s, i)).collect();
            let current: HashMap<Slot, usize> =
                self.rows[r].iter().enumerate().map(|(i, s)| (*s, i)).collect();

            let mut keyed: Vec<(f64, Slot)> = self.rows[r]
                .iter()
                .map(|&s| {
                    let mut ps: Vec<usize> = self
                        .links
                        .iter()
                        .filter_map(|(a, b)| {
                            let other = if *a == s {
                                *b
                            } else if *b == s {
                                *a
                            } else {
                                return None;
                            };
                            pos.get(&other).copied()
                        })
                        .collect();
                    ps.sort_unstable();
                    let median = if ps.is_empty() {
                        // Nothing to align with: hold position, so an
                        // unconnected node does not wander between runs.
                        current[&s] as f64
                    } else if ps.len() % 2 == 1 {
                        ps[ps.len() / 2] as f64
                    } else {
                        (ps[ps.len() / 2 - 1] + ps[ps.len() / 2]) as f64 / 2.0
                    };
                    (median, s)
                })
                .collect();

            keyed.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| slot_key(items, a.1).cmp(&slot_key(items, b.1)))
            });
            self.rows[r] = keyed.into_iter().map(|(_, s)| s).collect();
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

            let pairs: Vec<(usize, usize)> = self
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

            for (i, (u1, l1)) in pairs.iter().enumerate() {
                for (u2, l2) in pairs.iter().skip(i + 1) {
                    if (u1 < u2 && l1 > l2) || (u1 > u2 && l1 < l2) {
                        total += 1;
                    }
                }
            }
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
    fn assign_x(&mut self, items: &[Item]) {
        // A first pass left to right gives every slot a position.
        for row in &self.rows {
            let mut cursor = 0;
            for s in row {
                self.x.insert(*s, cursor);
                cursor += self.width[s] + HGAP;
            }
        }

        for _ in 0..4 {
            for down in [true, false] {
                let sequence: Vec<usize> = if down {
                    (1..self.rows.len()).collect()
                } else {
                    (0..self.rows.len().saturating_sub(1)).rev().collect()
                };
                for r in sequence {
                    // A folded line is held where the first pass left it, so
                    // the lines of one rank stay stacked as a block.
                    //
                    // Letting them sweep runs away instead: the lines of a fold
                    // have no links to each other, so a line takes its position
                    // from the rank below, packs its slots rightward from that
                    // one spot, and the rank below then recentres on where they
                    // landed — which is to the right of where it was. Each
                    // sweep repeats the shove and the fan walks off the page.
                    // Held still, the block is what the neighbouring rank
                    // centres itself on, which is the way round that settles.
                    if self.folded.contains(&r) {
                        continue;
                    }
                    let other = if down { r - 1 } else { r + 1 };
                    let want: Vec<(Slot, i32)> = self.rows[r]
                        .iter()
                        .map(|&s| {
                            let centres: Vec<i32> = self
                                .links
                                .iter()
                                .filter_map(|(a, b)| {
                                    let n = if *a == s {
                                        *b
                                    } else if *b == s {
                                        *a
                                    } else {
                                        return None;
                                    };
                                    self.rows[other]
                                        .contains(&n)
                                        .then(|| self.x[&n] + self.width[&n] / 2)
                                })
                                .collect();
                            let target = if centres.is_empty() {
                                self.x[&s]
                            } else {
                                let mut c = centres;
                                c.sort_unstable();
                                c[c.len() / 2] - self.width[&s] / 2
                            };
                            (s, target)
                        })
                        .collect();
                    self.pack(r, &want);
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

    /// Move each slot toward its target without letting any two overlap.
    fn pack(&mut self, row: usize, want: &[(Slot, i32)]) {
        let order = self.rows[row].clone();
        let target: HashMap<Slot, i32> = want.iter().copied().collect();

        // Left to right, never closer than HGAP to the previous.
        let mut cursor = i32::MIN;
        for s in &order {
            let w = self.width[s];
            let mut x = target.get(s).copied().unwrap_or(self.x[s]);
            if cursor != i32::MIN {
                x = x.max(cursor);
            }
            self.x.insert(*s, x);
            cursor = x + w + HGAP;
        }
        // Then right to left, so a node that was pushed right can pull its
        // left-hand neighbours along instead of stretching the row.
        let mut limit = i32::MAX;
        for s in order.iter().rev() {
            let w = self.width[s];
            let mut x = self.x[s];
            if limit != i32::MAX {
                x = x.min(limit - w - HGAP);
            }
            let t = target.get(s).copied().unwrap_or(x);
            if t > x && limit == i32::MAX {
                x = t;
            }
            self.x.insert(*s, x);
            limit = x;
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
            let ends = (rects[*a].center(), rects[*b].center());
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

fn segment_hits(p: Pt, q: Pt, b: Rect) -> bool {
    const STEPS: i32 = 48;
    for i in 1..STEPS {
        let t = i as f64 / STEPS as f64;
        let s = Pt {
            x: p.x + ((q.x - p.x) as f64 * t) as i32,
            y: p.y + ((q.y - p.y) as f64 * t) as i32,
        };
        if b.contains(s) {
            return true;
        }
    }
    false
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
