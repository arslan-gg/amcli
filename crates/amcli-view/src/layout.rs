//! Placing concepts on a new view.
//!
//! The default is Sugiyama layering over the dependency graph, not over the
//! ArchiMate layers. Ranking by layer is the obvious thing and it looks wrong:
//! most relationships in a real model are *within* a layer — component to
//! function, function to data object — so every one of them becomes a
//! horizontal line slicing through the boxes that happen to sit between its two
//! ends. Ranking by dependency puts those on consecutive rows instead, where
//! the edge is short and vertical and crosses nothing.
//!
//! Edges spanning more than one rank are routed through dummy nodes, which
//! reserve horizontal space in every row they pass and become the edge's
//! bendpoints. That is what stops a long edge from cutting through a box.
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
    /// The concept's ArchiMate layer, used only to break ties so that a
    /// business element tends to sit above an application one. It does not
    /// decide the row.
    pub rank: usize,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    /// Rows by dependency, long edges routed around what is in the way.
    Sugiyama,
    /// Rows strictly by ArchiMate layer. Truer to a layered viewpoint, and
    /// worse to look at whenever a layer talks to itself.
    Layers,
    /// Sorted into a square grid. Never pretty, never fails.
    Grid,
}

impl Algorithm {
    pub fn parse(s: &str) -> Option<Algorithm> {
        Some(match s {
            // `layered` maps to the good one deliberately: it is what people
            // type, and what they mean by it is "tidy rows", not "one row per
            // ArchiMate layer whatever that does to the lines".
            "sugiyama" | "auto" | "layered" => Algorithm::Sugiyama,
            "layers" => Algorithm::Layers,
            "grid" => Algorithm::Grid,
            _ => return None,
        })
    }
}

/// Where everything goes, and how the long edges get there.
#[derive(Clone, Debug, Default)]
pub struct Placement {
    /// One rectangle per item, in the order given.
    pub rects: Vec<Rect>,
    /// Absolute waypoints for an edge, by its index in the input. Only edges
    /// that need routing appear here.
    pub routes: HashMap<usize, Vec<Pt>>,
}

pub fn place(items: &[Item], edges: &[(usize, usize)], algo: Algorithm) -> Placement {
    match algo {
        Algorithm::Grid => Placement { rects: grid(items), routes: HashMap::new() },
        Algorithm::Layers => Placement { rects: by_layer(items, edges), routes: HashMap::new() },
        Algorithm::Sugiyama => sugiyama(items, edges),
    }
}

fn snap(v: i32) -> i32 {
    (v as f64 / GRID as f64).round() as i32 * GRID
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

fn key(items: &[Item], i: usize) -> (usize, &str, &str) {
    (items[i].rank, items[i].name.as_str(), items[i].id.as_str())
}

// ---- the two ranking strategies -------------------------------------------

/// Rows straight from the ArchiMate layer.
fn by_layer(items: &[Item], edges: &[(usize, usize)]) -> Vec<Rect> {
    let ranks: Vec<usize> = items.iter().map(|i| i.rank).collect();
    let normalized = compact(&ranks);
    let g = build(items, edges, &normalized, &[]);
    g.finish(items)
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
    (rank, reversed)
}

fn sugiyama(items: &[Item], edges: &[(usize, usize)]) -> Placement {
    let (raw_rank, reversed) = rank_by_dependency(items.len(), edges);
    let ranks = compact(&raw_rank);
    let g = build(items, edges, &ranks, &reversed.iter().copied().collect::<Vec<_>>());
    let rects = g.finish(items);
    let routes = g.routes(items, edges, &rects);
    Placement { rects, routes }
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

    let mut g = Layered { rows, width, height, links, x: HashMap::new(), y: Vec::new() };
    g.order(items);
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

    /// Waypoints for the edges that need them.
    fn routes(
        &self,
        items: &[Item],
        edges: &[(usize, usize)],
        rects: &[Rect],
    ) -> HashMap<usize, Vec<Pt>> {
        let mut out: HashMap<usize, Vec<Pt>> = HashMap::new();

        // A long edge follows its chain of dummies.
        for (r, row) in self.rows.iter().enumerate() {
            for s in row {
                let Slot::Dummy(ei, seg) = s else { continue };
                let p = Pt { x: self.x[s] + DUMMY_W / 2, y: self.y[r] + 27 };
                out.entry(*ei).or_default().push((*seg, p).1);
            }
        }
        // Rows are walked top to bottom, so the waypoints come out in order —
        // but an edge that runs upward needs them the other way round.
        for (ei, pts) in out.iter_mut() {
            let (a, b) = edges[*ei];
            if rects[a].y > rects[b].y {
                pts.reverse();
            }
        }

        // An edge inside one row would otherwise be a horizontal line straight
        // through whatever sits between its ends. Bow it below the row instead.
        for (ei, (a, b)) in edges.iter().enumerate() {
            if a == b || out.contains_key(&ei) {
                continue;
            }
            if rects[*a].y != rects[*b].y {
                continue;
            }
            let (left, right) = if rects[*a].x <= rects[*b].x { (*a, *b) } else { (*b, *a) };
            let gap = rects[right].x - (rects[left].x + rects[left].w);
            // Adjacent boxes need no help; a straight short line is fine.
            if gap <= HGAP + 8 {
                continue;
            }
            let y = rects[left].y + rects[left].h + 20 + (ei as i32 % 3) * 10;
            let x1 = rects[left].x + rects[left].w / 2;
            let x2 = rects[right].x + rects[right].w / 2;
            let mut pts = vec![Pt { x: x1, y }, Pt { x: x2, y }];
            if rects[*a].x > rects[*b].x {
                pts.reverse();
            }
            out.insert(ei, pts);
        }

        let _ = items;
        out
    }
}

fn slot_key(items: &[Item], s: Slot) -> (usize, String, String) {
    match s {
        Slot::Item(i) => (items[i].rank, items[i].name.clone(), items[i].id.clone()),
        // Dummies sort after real nodes at the same tie, and among themselves
        // by the edge they belong to, so ordering stays reproducible.
        Slot::Dummy(e, seg) => (usize::MAX, format!("~{e:08}"), format!("{seg:04}")),
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
            .map(|i| Item {
                id: format!("id{i}"),
                name: format!("Node {i}"),
                rank: i % 3,
                w: 120,
                h: 55,
            })
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
            .map(|(i, n)| Item { id: format!("i{i}"), name: n.to_string(), rank: 3, w: 120, h: 55 })
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

    #[test]
    fn a_long_edge_is_routed_rather_than_drawn_over_a_box() {
        // 0 -> 1 -> 2 with an extra 0 -> 2 that must skip a rank.
        let it = items(3);
        let edges = vec![(0, 1), (1, 2), (0, 2)];
        let p = place(&it, &edges, Algorithm::Sugiyama);

        let long = p.routes.get(&2).expect("the skipping edge gets waypoints");
        assert!(!long.is_empty());
        // The waypoint sits beside the box it passes, not on top of it.
        for w in long {
            assert!(!p.rects[1].contains(*w), "waypoint {w:?} is inside box 1");
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
            .map(|i| Item { id: format!("i{i}"), name: format!("N{i}"), rank: 0, w: 120, h: 55 })
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
        for algo in [Algorithm::Grid, Algorithm::Layers, Algorithm::Sugiyama] {
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
        for algo in [Algorithm::Grid, Algorithm::Layers, Algorithm::Sugiyama] {
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

    #[test]
    fn the_layers_algorithm_still_puts_one_layer_per_row() {
        let it = items(9);
        let p = place(&it, &[], Algorithm::Layers);
        for (i, item) in it.iter().enumerate() {
            for (j, other) in it.iter().enumerate() {
                if item.rank < other.rank {
                    assert!(p.rects[i].y < p.rects[j].y);
                }
            }
        }
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
