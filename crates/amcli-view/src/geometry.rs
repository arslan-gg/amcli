//! The geometry kernel.
//!
//! This is the single implementation of what the numbers in a view mean.
//! Rendering and format conversion both read it, because two implementations
//! would drift and then an exported diagram and a rendered one would disagree.

/// A rectangle in absolute view coordinates. Negative origins are normal: an
/// Archi canvas extends left and up of zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Draw2D's `Rectangle.getCenter`, integer division included. This is the
    /// reference point bendpoints interpolate between, so rounding it any other
    /// way moves every routed line.
    pub fn center(self) -> Pt {
        Pt { x: self.x + self.w / 2, y: self.y + self.h / 2 }
    }

    pub fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = (self.x + self.w).max(other.x + other.w);
        let b = (self.y + self.h).max(other.y + other.h);
        Rect { x, y, w: r - x, h: b - y }
    }

    pub fn contains(self, p: Pt) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Pt {
    pub x: i32,
    pub y: i32,
}

/// A bendpoint as stored: offsets from the source and target reference points
/// rather than a position. This is the legacy Draw2D `RelativeBendpoint` model,
/// and it is why a stored bendpoint survives moving either end.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Bendpoint {
    pub start_x: i32,
    pub start_y: i32,
    pub end_x: i32,
    pub end_y: i32,
}

/// Where a connection actually runs, end to end.
///
/// `bendpoints` are interpolated first, because the polyline's terminal points
/// depend on them: the source end aims at the first bendpoint, not at the
/// target. With no bendpoints the two ends aim at each other.
pub fn route(source: Rect, target: Rect, bendpoints: &[Bendpoint]) -> Vec<Pt> {
    let ref_s = source.center();
    let ref_t = target.center();
    let n = bendpoints.len();

    let mid: Vec<Pt> = bendpoints
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // Draw2D weights the i-th of n points at (i+1)/(n+1).
            let w = (i + 1) as f64 / (n + 1) as f64;
            let sx = (ref_s.x + b.start_x) as f64;
            let sy = (ref_s.y + b.start_y) as f64;
            let tx = (ref_t.x + b.end_x) as f64;
            let ty = (ref_t.y + b.end_y) as f64;
            Pt {
                x: (sx * (1.0 - w) + tx * w).round() as i32,
                y: (sy * (1.0 - w) + ty * w).round() as i32,
            }
        })
        .collect();

    let aim_from_source = mid.first().copied().unwrap_or(ref_t);
    let aim_from_target = mid.last().copied().unwrap_or(ref_s);

    let mut out = Vec::with_capacity(n + 2);
    out.push(chopbox(source, aim_from_source));
    out.extend(mid);
    out.push(chopbox(target, aim_from_target));
    out
}

/// The inverse: what to store so that a bendpoint lands exactly on `p`.
///
/// The system is underdetermined — four unknowns, two equations — and this is
/// the canonical solution. It is exact for every weight, so the position does
/// not depend on how many other bendpoints the connection has, and the point
/// drifts sensibly when either end is later moved.
pub fn bendpoint_for(source: Rect, target: Rect, p: Pt) -> Bendpoint {
    let s = source.center();
    let t = target.center();
    Bendpoint { start_x: p.x - s.x, start_y: p.y - s.y, end_x: p.x - t.x, end_y: p.y - t.y }
}

/// Where a line aimed at `toward` meets the border of `b`.
///
/// A port of Draw2D's `ChopboxAnchor.getLocation`, expansion quirk and all: the
/// box is translated by (-1,-1) and grown by 1 before the intersection, which
/// puts the anchor half a pixel outside the figure. Archi's own output has that
/// offset in it, so reproducing it costs nothing and matching it is free.
pub fn chopbox(b: Rect, toward: Pt) -> Pt {
    let (rx, ry) = (b.x as f64 - 1.0, b.y as f64 - 1.0);
    let (rw, rh) = (b.w as f64 + 1.0, b.h as f64 + 1.0);
    let cx = rx + 0.5 * rw;
    let cy = ry + 0.5 * rh;

    // A zero-sized figure, or a target sitting exactly on the centre, has no
    // intersection to compute.
    if rw <= 0.0 || rh <= 0.0 || (toward.x == cx as i32 && toward.y == cy as i32) {
        return Pt { x: cx as i32, y: cy as i32 };
    }

    let dx = toward.x as f64 - cx;
    let dy = toward.y as f64 - cy;
    let scale = 0.5 / (dx.abs() / rw).max(dy.abs() / rh);
    Pt { x: (cx + dx * scale).round() as i32, y: (cy + dy * scale).round() as i32 }
}

/// Archi's default figure sizes, used when bounds carry `-1`.
///
/// Verified against Archi 5.9: `getDefaultSizeForFigureType` no longer varies by
/// figure type — every override delegates to super — so one size covers every
/// element regardless of its alternate figure.
pub const ELEMENT_SIZE: (i32, i32) = (120, 55);
pub const JUNCTION_SIZE: (i32, i32) = (15, 15);
pub const NOTE_SIZE: (i32, i32) = (185, 80);
pub const GROUP_SIZE: (i32, i32) = (400, 140);
pub const IMAGE_SIZE: (i32, i32) = (200, 150);

/// The tab band across the top of a Group.
pub const GROUP_HEADER: i32 = 18;

/// The corner Archi cuts off a Note.
pub const NOTE_DOG_EAR: i32 = 13;

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_chopbox_anchor_lands_on_the_border() {
        let b = r(100, 100, 120, 55);
        for toward in [
            Pt { x: 500, y: 127 },
            Pt { x: -100, y: 127 },
            Pt { x: 160, y: -50 },
            Pt { x: 160, y: 900 },
        ] {
            let p = chopbox(b, toward);
            let expanded = r(b.x - 1, b.y - 1, b.w + 1, b.h + 1);
            assert!(expanded.contains(p), "{p:?} is outside {expanded:?}");
            let on_edge = p.x <= expanded.x + 1
                || p.x >= expanded.x + expanded.w - 1
                || p.y <= expanded.y + 1
                || p.y >= expanded.y + expanded.h - 1;
            assert!(on_edge, "{p:?} is not on the border of {expanded:?}");
        }
    }

    #[test]
    fn a_degenerate_figure_does_not_divide_by_zero() {
        assert_eq!(chopbox(r(10, 10, 0, 0), Pt { x: 9, y: 9 }), Pt { x: 9, y: 9 });
    }

    /// The property that pins the whole bendpoint model: writing a point and
    /// reading it back returns the same point, whatever else the connection has.
    #[test]
    fn a_bendpoint_round_trips_exactly() {
        let s = r(0, 0, 120, 55);
        let t = r(400, 300, 120, 55);
        for p in [Pt { x: 200, y: 100 }, Pt { x: -50, y: 900 }, Pt { x: 0, y: 0 }] {
            let b = bendpoint_for(s, t, p);
            for n in 1..5 {
                let mut all = vec![Bendpoint::default(); n];
                for slot in 0..n {
                    all[slot] = b;
                    let routed = route(s, t, &all);
                    assert_eq!(routed[1 + slot], p, "n={n} slot={slot}");
                }
            }
        }
    }

    #[test]
    fn routing_is_translation_invariant() {
        let s = r(0, 0, 120, 55);
        let t = r(400, 300, 120, 55);
        let bp = [Bendpoint { start_x: 50, start_y: -20, end_x: -50, end_y: 20 }];
        let base = route(s, t, &bp);

        let (dx, dy) = (137, -409);
        let moved = route(r(s.x + dx, s.y + dy, s.w, s.h), r(t.x + dx, t.y + dy, t.w, t.h), &bp);
        for (a, b) in base.iter().zip(moved.iter()) {
            assert_eq!((b.x - a.x, b.y - a.y), (dx, dy));
        }
    }

    #[test]
    fn a_connection_with_no_bendpoints_runs_between_the_two_borders() {
        let s = r(0, 0, 120, 55);
        let t = r(400, 0, 120, 55);
        let p = route(s, t, &[]);
        assert_eq!(p.len(), 2);
        assert!(p[0].x > 100 && p[0].x < 130, "leaves the source's right edge: {p:?}");
        assert!(p[1].x > 390 && p[1].x < 410, "arrives at the target's left edge: {p:?}");
    }

    /// A self-loop with no bendpoints collapses to a point. That is genuinely
    /// what Archi draws, so it is reproduced rather than quietly improved.
    #[test]
    fn a_self_loop_without_bendpoints_is_degenerate_exactly_as_archi_draws_it() {
        let s = r(0, 0, 120, 55);
        let p = route(s, s, &[]);
        assert_eq!(p[0], p[1]);
    }

    #[test]
    fn rectangles_union_and_centre_predictably() {
        assert_eq!(r(0, 0, 10, 10).union(r(20, 20, 10, 10)), r(0, 0, 30, 30));
        assert_eq!(r(0, 0, 121, 55).center(), Pt { x: 60, y: 27 }, "integer division, as Draw2D");
    }
}
