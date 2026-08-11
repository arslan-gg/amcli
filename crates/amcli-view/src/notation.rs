//! Archi's visual constants, ported from its MIT sources.
//!
//! These are facts about how Archi draws, not choices. Where a number came from
//! a specific class it is named, because the next person to doubt one of these
//! should be able to check it.

use amcli_model::{ElementType, Layer, RelType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// `ColorFactory.getDerivedLineColor`, contrast factor 0.7, integer
    /// truncation. Archi derives an element's border from its fill by default
    /// (`DERIVE_ELEMENT_LINE_COLOR`), so a renderer that draws black borders
    /// looks wrong on every diagram.
    pub fn derived_line(self) -> Rgb {
        let f = |c: u8| (c as f32 * 0.7) as u8;
        Rgb(f(self.0), f(self.1), f(self.2))
    }

    /// `ColorFactory.getDarkerColor`, used for a Group's header band.
    pub fn darker(self) -> Rgb {
        let f = |c: u8| (c as f32 * 0.9) as u8;
        Rgb(f(self.0), f(self.1), f(self.2))
    }
}

/// Default fill per layer, from `AbstractArchimateElementUIProvider`.
pub fn layer_fill(layer: Layer) -> Rgb {
    match layer {
        Layer::Strategy => Rgb(245, 222, 170),
        Layer::Business => Rgb(255, 255, 181),
        Layer::Application => Rgb(181, 255, 255),
        Layer::Technology | Layer::Physical => Rgb(201, 231, 183),
        Layer::Motivation => Rgb(204, 204, 255),
        Layer::ImplementationMigration => Rgb(255, 224, 224),
        Layer::Other => Rgb(255, 255, 255),
    }
}

/// Connections, notes and groups use this rather than a derived colour.
pub const DEFAULT_LINE: Rgb = Rgb(92, 92, 92);
pub const WHITE: Rgb = Rgb(255, 255, 255);
pub const BLACK: Rgb = Rgb(0, 0, 0);

/// The outline an element gets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Figure {
    /// Structure and passive elements.
    Rect,
    /// Behaviour elements: process, function, interaction, service, event.
    RoundedRect,
    /// Motivation elements.
    Octagon,
    /// Junction.
    Circle,
    /// Grouping and the visual Group container.
    Tabbed,
    /// A note, with its corner cut.
    Note,
}

/// Behaviour elements are drawn with rounded corners; motivation elements as
/// octagons; everything else square. Deriving this from the type name rather
/// than a 61-row table keeps it honest: the naming *is* the rule in ArchiMate.
pub fn figure_of(e: ElementType) -> Figure {
    let info = e.info();
    if e == ElementType::Junction {
        return Figure::Circle;
    }
    if e == ElementType::Grouping {
        return Figure::Tabbed;
    }
    if info.layer == Layer::Motivation {
        return Figure::Octagon;
    }
    let n = info.short;
    let behaviour = ["Process", "Function", "Interaction", "Service", "Event", "ValueStream"]
        .iter()
        .any(|suffix| n.ends_with(suffix));
    if behaviour { Figure::RoundedRect } else { Figure::Rect }
}

/// What is drawn at the end of a connection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Deco {
    None,
    /// Composition.
    FilledDiamond,
    /// Aggregation.
    HollowDiamond,
    /// Triggering, assignment.
    FilledArrow,
    /// Serving, access.
    OpenArrow,
    /// Realization, specialization.
    HollowTriangle,
    /// Assignment's source end.
    Ball,
    /// A directed association.
    HalfArrow,
}

/// Line style and end decorations per relationship type, ported from the
/// `*ConnectionFigure` classes.
pub struct RelStyle {
    pub dash: Option<&'static str>,
    pub source: Deco,
    pub target: Deco,
}

pub fn rel_style(r: RelType, access: Option<i64>, directed: bool) -> RelStyle {
    match r {
        RelType::Composition => {
            RelStyle { dash: None, source: Deco::FilledDiamond, target: Deco::None }
        }
        RelType::Aggregation => {
            RelStyle { dash: None, source: Deco::HollowDiamond, target: Deco::None }
        }
        RelType::Assignment => {
            RelStyle { dash: None, source: Deco::Ball, target: Deco::FilledArrow }
        }
        RelType::Realization => {
            RelStyle { dash: Some("2 2"), source: Deco::None, target: Deco::HollowTriangle }
        }
        RelType::Specialization => {
            RelStyle { dash: None, source: Deco::None, target: Deco::HollowTriangle }
        }
        RelType::Triggering => {
            RelStyle { dash: None, source: Deco::None, target: Deco::FilledArrow }
        }
        RelType::Flow => {
            RelStyle { dash: Some("6 3"), source: Deco::None, target: Deco::FilledArrow }
        }
        RelType::Serving => RelStyle { dash: None, source: Deco::None, target: Deco::OpenArrow },
        RelType::Access => {
            // accessType 0 is WRITE, 1 READ, 2 UNSPECIFIED, 3 READ/WRITE. The
            // arrow points the way the data moves, so read reverses it.
            let a = access.unwrap_or(0);
            RelStyle {
                dash: Some("2 2"),
                source: if a == 1 || a == 3 { Deco::OpenArrow } else { Deco::None },
                target: if a == 0 || a == 3 { Deco::OpenArrow } else { Deco::None },
            }
        }
        RelType::Influence => {
            RelStyle { dash: Some("6 3"), source: Deco::None, target: Deco::OpenArrow }
        }
        RelType::Association => RelStyle {
            dash: None,
            source: Deco::None,
            target: if directed { Deco::HalfArrow } else { Deco::None },
        },
    }
}

/// A decoration as a polygon or polyline in decoration-local coordinates, where
/// the tip is at the origin and the line runs off along +x. Scales are the ones
/// Draw2D applies to each template.
pub fn deco_points(d: Deco) -> (&'static [(f64, f64)], bool) {
    match d {
        // PathDrawnPolygonDecoration template scaled 5x3.
        Deco::FilledDiamond | Deco::HollowDiamond => {
            (&[(0.0, 0.0), (-10.0, 6.0), (-20.0, 0.0), (-10.0, -6.0)], true)
        }
        // Draw2D PolygonDecoration TRIANGLE_TIP at the default 7x3.
        Deco::FilledArrow => (&[(0.0, 0.0), (-7.0, 3.0), (-7.0, -3.0)], true),
        // PolylineDecoration: two strokes, not a filled shape.
        Deco::OpenArrow => (&[(-7.0, 3.0), (0.0, 0.0), (-7.0, -3.0)], false),
        Deco::HollowTriangle => (&[(0.0, 0.0), (-10.0, 7.0), (-10.0, -7.0)], true),
        Deco::HalfArrow => (&[(0.0, 0.0), (-9.0, -4.0)], false),
        Deco::Ball | Deco::None => (&[], false),
    }
}

/// Radius of the assignment relationship's source ball.
pub const BALL_RADIUS: f64 = 3.0;
