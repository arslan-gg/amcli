//! Placing concepts on a new view.
//!
//! Everything here is deterministic by construction: no randomness, no seeds,
//! no iteration over a hash map, ties broken by `(name, id)`, all coordinates
//! integers snapped to the grid. Two runs on the same input produce identical
//! bytes, and a shuffled input produces the same layout — both are asserted in
//! the tests, because in Rust a `HashMap` iteration order is deliberately
//! randomised and one stray `for (k, v) in &map` would silently break it.

use crate::geometry::Rect;

/// Grid the output snaps to. Archi's own bounds are integers anyway, so float
/// drift would only add diff noise.
pub const GRID: i32 = 12;

const HGAP: i32 = 30;
const VGAP: i32 = 60;

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    /// Which row this belongs in. ArchiMate hands this over for free: the
    /// layer *is* the rank, so there is no ranking pass and no cycle-breaking.
    pub rank: usize,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    /// Rows by ArchiMate layer, ordered to reduce crossings.
    Layered,
    /// Sorted into a square grid. Never pretty, never fails.
    Grid,
}

impl Algorithm {
    pub fn parse(s: &str) -> Option<Algorithm> {
        Some(match s {
            "layered" => Algorithm::Layered,
            "grid" => Algorithm::Grid,
            _ => return None,
        })
    }
}

/// Place items, returning one rectangle per item in the order given.
pub fn place(items: &[Item], edges: &[(usize, usize)], algo: Algorithm) -> Vec<Rect> {
    match algo {
        Algorithm::Grid => grid(items),
        Algorithm::Layered => layered(items, edges),
    }
}

fn snap(v: i32) -> i32 {
    (v as f64 / GRID as f64).round() as i32 * GRID
}

fn grid(items: &[Item]) -> Vec<Rect> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| {
        (items[*a].rank, &items[*a].name, &items[*a].id).cmp(&(
            items[*b].rank,
            &items[*b].name,
            &items[*b].id,
        ))
    });

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

/// Sugiyama with the ranking already done.
///
/// Ordering within a rank is four median sweeps — down, up, down, up — keeping
/// whichever pass crossed least. That is a heuristic, not an optimum, but it is
/// cheap and it is stable.
fn layered(items: &[Item], edges: &[(usize, usize)]) -> Vec<Rect> {
    let max_rank = items.iter().map(|i| i.rank).max().unwrap_or(0);
    let mut ranks: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, item) in items.iter().enumerate() {
        ranks[item.rank].push(i);
    }
    for r in ranks.iter_mut() {
        r.sort_by(|a, b| (&items[*a].name, &items[*a].id).cmp(&(&items[*b].name, &items[*b].id)));
    }

    let mut best = ranks.clone();
    let mut best_crossings = crossings(&ranks, edges);
    for sweep in 0..4 {
        let down = sweep % 2 == 0;
        median_pass(&mut ranks, items, edges, down);
        let c = crossings(&ranks, edges);
        // Strictly better only, so the earliest sweep wins a tie and the result
        // does not depend on how many sweeps we happen to run.
        if c < best_crossings {
            best_crossings = c;
            best = ranks.clone();
        }
    }

    let row_h: Vec<i32> =
        best.iter().map(|r| r.iter().map(|i| items[*i].h).max().unwrap_or(55)).collect();

    let mut out = vec![Rect::default(); items.len()];
    let mut y = 0;
    for (r, row) in best.iter().enumerate() {
        let mut x = 0;
        for &i in row {
            out[i] = Rect { x: snap(x), y: snap(y), w: items[i].w, h: items[i].h };
            x += items[i].w + HGAP;
        }
        y += row_h[r] + VGAP;
    }
    out
}

fn median_pass(ranks: &mut [Vec<usize>], items: &[Item], edges: &[(usize, usize)], down: bool) {
    let order = |ranks: &[Vec<usize>], node: usize| -> Option<usize> {
        ranks[items[node].rank].iter().position(|n| *n == node)
    };

    let sequence: Vec<usize> = if down {
        (1..ranks.len()).collect()
    } else {
        (0..ranks.len().saturating_sub(1)).rev().collect()
    };

    for r in sequence {
        let snapshot = ranks.to_vec();
        let mut keyed: Vec<(f64, usize)> = ranks[r]
            .iter()
            .map(|&n| {
                let mut positions: Vec<usize> = edges
                    .iter()
                    .filter_map(|(a, b)| {
                        let other = if *a == n {
                            *b
                        } else if *b == n {
                            *a
                        } else {
                            return None;
                        };
                        let want = if down { r.checked_sub(1)? } else { r + 1 };
                        (items[other].rank == want).then_some(other)
                    })
                    .filter_map(|o| order(&snapshot, o))
                    .collect();
                positions.sort_unstable();
                let median = if positions.is_empty() {
                    // Nothing to align with: keep the current position so an
                    // unconnected node does not wander between runs.
                    order(&snapshot, n).unwrap_or(0) as f64
                } else {
                    positions[positions.len() / 2] as f64
                };
                (median, n)
            })
            .collect();

        keyed.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Ties break on the concept, never on iteration order.
                .then_with(|| {
                    (&items[a.1].name, &items[a.1].id).cmp(&(&items[b.1].name, &items[b.1].id))
                })
        });
        ranks[r] = keyed.into_iter().map(|(_, n)| n).collect();
    }
}

/// Edge crossings between adjacent ranks, counted pairwise.
fn crossings(ranks: &[Vec<usize>], edges: &[(usize, usize)]) -> usize {
    let mut pos = std::collections::HashMap::new();
    for row in ranks {
        for (i, n) in row.iter().enumerate() {
            pos.insert(*n, i);
        }
    }
    let mut count = 0;
    for (i, (a1, b1)) in edges.iter().enumerate() {
        for (a2, b2) in edges.iter().skip(i + 1) {
            let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                (pos.get(a1), pos.get(b1), pos.get(a2), pos.get(b2))
            else {
                continue;
            };
            if (x1 < x2 && y1 > y2) || (x1 > x2 && y1 < y2) {
                count += 1;
            }
        }
    }
    count
}

/// Somewhere to put a new object without disturbing anything already placed.
///
/// This is what makes `--only-new` the default: existing objects are pinned, so
/// adding one element does not produce a four-hundred-line diff.
pub fn free_slot(taken: &[Rect], w: i32, h: i32) -> Rect {
    let step = GRID * 4;
    for row in 0..500 {
        for col in 0..40 {
            let candidate =
                Rect { x: col * (w + HGAP).max(step), y: row * (h + VGAP).max(step), w, h };
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

    #[test]
    fn layout_is_reproducible() {
        let it = items(12);
        let edges = vec![(0, 1), (1, 2), (3, 4), (0, 5), (6, 7), (8, 9)];
        for algo in [Algorithm::Grid, Algorithm::Layered] {
            assert_eq!(place(&it, &edges, algo), place(&it, &edges, algo), "{algo:?}");
        }
    }

    /// The test that catches an accidental dependency on hash iteration order.
    #[test]
    fn layout_does_not_depend_on_input_order() {
        let it = items(9);
        let edges = vec![(0, 1), (1, 2), (3, 4)];
        let a = place(&it, &edges, Algorithm::Layered);

        // Reverse the items and remap the edges, then compare per concept.
        let mut reversed = it.clone();
        reversed.reverse();
        let remap = |i: usize| it.len() - 1 - i;
        let redges: Vec<(usize, usize)> =
            edges.iter().map(|(x, y)| (remap(*x), remap(*y))).collect();
        let b = place(&reversed, &redges, Algorithm::Layered);

        for (i, item) in it.iter().enumerate() {
            let j = reversed.iter().position(|r| r.id == item.id).unwrap();
            assert_eq!(a[i], b[j], "{} moved", item.id);
        }
    }

    #[test]
    fn everything_lands_on_the_grid_and_nothing_overlaps() {
        let it = items(10);
        for algo in [Algorithm::Grid, Algorithm::Layered] {
            let out = place(&it, &[], algo);
            for r in &out {
                assert_eq!(r.x % GRID, 0, "{r:?}");
                assert_eq!(r.y % GRID, 0, "{r:?}");
            }
            for (i, a) in out.iter().enumerate() {
                for b in out.iter().skip(i + 1) {
                    let overlap =
                        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                    assert!(!overlap, "{a:?} overlaps {b:?} under {algo:?}");
                }
            }
        }
    }

    #[test]
    fn ranks_become_rows() {
        let it = items(9);
        let out = place(&it, &[], Algorithm::Layered);
        for (i, item) in it.iter().enumerate() {
            for (j, other) in it.iter().enumerate() {
                if item.rank < other.rank {
                    assert!(
                        out[i].y < out[j].y,
                        "rank {} should sit above {}",
                        item.rank,
                        other.rank
                    );
                }
            }
        }
    }

    #[test]
    fn a_free_slot_avoids_what_is_already_placed() {
        let taken = vec![Rect { x: 0, y: 0, w: 120, h: 55 }, Rect { x: 200, y: 0, w: 120, h: 55 }];
        let slot = free_slot(&taken, 120, 55);
        for t in &taken {
            let overlap = slot.x < t.x + t.w
                && t.x < slot.x + slot.w
                && slot.y < t.y + t.h
                && t.y < slot.y + slot.h;
            assert!(!overlap, "{slot:?} clashes with {t:?}");
        }
    }
}
