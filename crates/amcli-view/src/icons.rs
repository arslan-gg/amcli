//! The type icon Archi draws in the top-right corner of an element figure.
//!
//! Archi draws these in Java (`*Figure.drawIcon` and the `IconicDelegate`
//! classes), so there is nothing to vendor: each entry here is that drawing
//! ported by hand into SVG path data on a fixed [`ICON_BOX`]-square canvas,
//! stroke-only, `currentColor`, so the renderer colours it with the figure's
//! line colour the way Archi does. They are code, not `assets/archi` inputs.
//!
//! Every coordinate is non-negative and inside the box. That is what lets the
//! renderer emit `<symbol>`s once and place them with `<use>`, and it keeps
//! `-0` out of the output, which the byte-stability test forbids.

use amcli_model::ElementType;

/// Side of the square an icon is drawn on, in view units.
pub const ICON_BOX: i32 = 16;

/// Where the icon's top-left corner sits relative to the figure's top-right
/// corner: `(right - ICON_RIGHT, top + ICON_TOP)`. Archi's figures put the icon
/// a few pixels in from the top-right; `layout::ICON_INSET` already reserves
/// this much room off the label.
pub const ICON_RIGHT: i32 = 20;
pub const ICON_TOP: i32 = 4;

/// SVG path data for the icon of an element type, or `None` when Archi draws
/// none (a Junction is its own icon).
pub fn icon(e: ElementType) -> Option<&'static str> {
    use ElementType as T;
    Some(match e {
        // Strategy — ResourceFigure: a battery on its side.
        T::Resource => "M1 4H13V12H1ZM13 6H15V10H13ZM3.5 6V10M6 6V10M8.5 6V10",
        // CapabilityFigure: three steps.
        T::Capability => {
            "M0.5 15.5V10.5H5.5V5.5H10.5V0.5H15.5V15.5ZM5.5 15.5V10.5M10.5 15.5V5.5M5.5 10.5H15.5M10.5 5.5H15.5"
        }
        // CourseOfActionFigure: a target with an arrow hitting the bull.
        T::CourseOfAction => {
            "M8 8m-6 0a6 6 0 1 0 12 0a6 6 0 1 0 -12 0M8 8m-3 0a3 3 0 1 0 6 0a3 3 0 1 0 -6 0M2 14L8 8M8 8L5 9M8 8L7 11"
        }
        // ValueStreamFigure: a chevron.
        T::ValueStream => "M1 3H11L15 8L11 13H1L5 8Z",

        // Business — BusinessActorFigure: a stick figure.
        T::BusinessActor => {
            "M6 3a2 2 0 1 0 4 0a2 2 0 1 0 -4 0M8 5V10M4 7H12M8 10L4.5 15M8 10L11.5 15"
        }
        // BusinessRoleFigure / StakeholderFigure: a cylinder on its side.
        T::BusinessRole | T::Stakeholder => {
            "M3 5H12A3 3.5 0 0 1 12 12H3A2 3.5 0 0 1 3 5ZM12 5A3 3.5 0 0 0 12 12"
        }
        // CollaborationFigure: two overlapping circles.
        T::BusinessCollaboration | T::ApplicationCollaboration | T::TechnologyCollaboration => {
            "M5.5 8m-4.5 0a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0M10.5 8m-4.5 0a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0"
        }
        // InterfaceFigure: a lollipop.
        T::BusinessInterface | T::ApplicationInterface | T::TechnologyInterface => {
            "M1 8H8M8 8a3 3 0 1 0 6 0a3 3 0 1 0 -6 0"
        }
        // ProcessFigure: a fat arrow.
        T::BusinessProcess | T::ApplicationProcess | T::TechnologyProcess => {
            "M1 5H9V2L15 8L9 14V11H1Z"
        }
        // FunctionFigure: a chevron pointing up with a notch below.
        T::BusinessFunction | T::ApplicationFunction | T::TechnologyFunction => {
            "M1 6L8 1L15 6V15L8 10L1 15Z"
        }
        // InteractionFigure: a circle split down the middle.
        T::BusinessInteraction | T::ApplicationInteraction | T::TechnologyInteraction => {
            "M6.5 2A6 6 0 0 0 6.5 14M9.5 2A6 6 0 0 1 9.5 14"
        }
        // EventFigure: notched on the left, rounded on the right.
        T::BusinessEvent | T::ApplicationEvent | T::TechnologyEvent | T::ImplementationEvent => {
            "M1 3H11A5 5 0 0 1 11 13H1L4 8Z"
        }
        // ServiceFigure: a pill.
        T::BusinessService | T::ApplicationService | T::TechnologyService => {
            "M5 4H11A4 4 0 0 1 11 12H5A4 4 0 0 1 5 4Z"
        }
        // ObjectFigure: a rectangle with a header band.
        T::BusinessObject | T::DataObject => "M1 3H15V13H1ZM1 6H15",
        // ContractFigure: two bands.
        T::Contract => "M1 3H15V13H1ZM1 6H15M1 10H15",
        // RepresentationFigure / DeliverableFigure: a wavy bottom edge.
        T::Representation | T::Deliverable => "M1 3H15V12C12 10 11 14 8 12S4 10 1 12Z",
        // ProductFigure: a rectangle with a tab in the corner.
        T::Product => "M1 3H15V13H1ZM1 7H8V3",

        // Application — ApplicationComponentFigure: a box with two tabs.
        T::ApplicationComponent => "M4 4.5V2H14V14H4V11.5M4 9V7M1 4.5H7V7H1ZM1 9H7V11.5H1Z",

        // Technology — NodeFigure: a box in three-quarter view.
        T::Node => "M1 5L4 2H15V11L12 14H1ZM1 5H12V14M12 5L15 2",
        // DeviceFigure: a screen on a stand.
        T::Device => "M2 2H14V10H2ZM4 13H12M6 10L4 13M10 10L12 13",
        // SystemSoftwareFigure: a disc, one circle over another.
        T::SystemSoftware => {
            "M7 9m-6 0a6 6 0 1 0 12 0a6 6 0 1 0 -12 0M10 6m-4.5 0a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0"
        }
        // PathFigure: a dashed line arrowed at both ends.
        T::Path => "M1 8L4 5M1 8L4 11M15 8L12 5M15 8L12 11M3 8H5M6.5 8H9.5M11 8H13",
        // CommunicationNetworkFigure: two nodes joined by a line.
        T::CommunicationNetwork => {
            "M3 11m-2 0a2 2 0 1 0 4 0a2 2 0 1 0 -4 0M13 5m-2 0a2 2 0 1 0 4 0a2 2 0 1 0 -4 0M4.5 9.5L11.5 6.5M5 11H12M4 5H11"
        }
        // ArtifactFigure: a page with the corner folded.
        T::Artifact => "M2 1H10L14 5V15H2ZM10 1V5H14",
        // EquipmentFigure: two gears.
        T::Equipment => {
            "M5.5 10.5m-3 0a3 3 0 1 0 6 0a3 3 0 1 0 -6 0M5.5 6V7.5M5.5 13.5V15M1 10.5H2.5M8.5 10.5H10M2.3 7.3L3.4 8.4M7.6 12.6L8.7 13.7M2.3 13.7L3.4 12.6M7.6 8.4L8.7 7.3M11.5 5m-2.2 0a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0M11.5 1.5V2.8M11.5 7.2V8.5M8 5H9.3M13.7 5H15M9 2.5L10 3.5M13 6.5L14 7.5M9 7.5L10 6.5M13 3.5L14 2.5"
        }
        // FacilityFigure: a factory roofline with a chimney.
        T::Facility => "M1 15V7L5 10V7L9 10V7L12 9.5V2H15V15Z",
        // DistributionNetworkFigure: a double line arrowed at both ends.
        T::DistributionNetwork => "M1 8L4 5M1 8L4 11M15 8L12 5M15 8L12 11M3 6H13M3 10H13",
        // MaterialFigure: a hexagon.
        T::Material => "M4.5 2.5H11.5L15 8L11.5 13.5H4.5L1 8ZM5.5 5.5H10.5M5.5 10.5H10.5",

        // Motivation — DriverFigure: a wheel with spokes.
        T::Driver => {
            "M8 8m-4.5 0a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0M8 1V15M1 8H15M3 3L13 13M13 3L3 13M8 8m-1.5 0a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0"
        }
        // AssessmentFigure: a magnifying glass.
        T::Assessment => "M10 6m-4.5 0a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0M6.8 9.2L1.5 14.5",
        // GoalFigure: a bull's-eye.
        T::Goal => {
            "M8 8m-6 0a6 6 0 1 0 12 0a6 6 0 1 0 -12 0M8 8m-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0M8 8m-1 0a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"
        }
        // OutcomeFigure: a bull's-eye with an arrow in it.
        T::Outcome => {
            "M8 8m-6 0a6 6 0 1 0 12 0a6 6 0 1 0 -12 0M8 8m-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0M8 8L14.5 1.5M14.5 1.5V5M14.5 1.5H11"
        }
        // PrincipleFigure: an exclamation mark in a box.
        T::Principle => "M2 1.5H14V14.5H2ZM8 4V9.5M8 11.5V12.5",
        // RequirementFigure: a parallelogram.
        T::Requirement => "M4 3H15L12 13H1Z",
        // ConstraintFigure: a parallelogram with a bar.
        T::Constraint => "M4 3H15L12 13H1ZM6.5 3L3.5 13",
        // MeaningFigure: a cloud.
        T::Meaning => "M4 12.5A3 3 0 0 1 3.5 6.6A4 4 0 0 1 11.2 5.2A3.2 3.2 0 0 1 12.5 12.5Z",
        // ValueFigure: an ellipse.
        T::Value => "M8 8m-7 0a7 4.5 0 1 0 14 0a7 4.5 0 1 0 -14 0",

        // Implementation & migration — WorkPackageFigure: a rounded rectangle.
        T::WorkPackage => {
            "M3 3H13A2.5 2.5 0 0 1 15.5 5.5V10.5A2.5 2.5 0 0 1 13 13H3A2.5 2.5 0 0 1 0.5 10.5V5.5A2.5 2.5 0 0 1 3 3Z"
        }
        // PlateauFigure: three bars, stepped.
        T::Plateau => "M4 3H15V5H4ZM2.5 7H13.5V9H2.5ZM1 11H12V13H1Z",
        // GapFigure: a circle crossed by two lines.
        T::Gap => "M8 8m-5 0a5 5 0 1 0 10 0a5 5 0 1 0 -10 0M1 6.5H15M1 9.5H15",

        // Other — LocationFigure: a map pin.
        T::Location => {
            "M8 15C8 15 3 9 3 6A5 5 0 0 1 13 6C13 9 8 15 8 15ZM8 6m-1.8 0a1.8 1.8 0 1 0 3.6 0a1.8 1.8 0 1 0 -3.6 0"
        }
        // GroupingFigure draws no corner icon — the figure is the tab — but
        // the graph shows one, so it gets the tab in miniature.
        T::Grouping => "M1 5V14H15V5H8V2H1ZM1 5H8",
        T::Junction => return None,
    })
}

/// The `<symbol>` an icon becomes: `id="i-{Type}"`, drawn in `currentColor`.
pub fn symbol(e: ElementType) -> Option<String> {
    let d = icon(e)?;
    Some(format!(
        "<symbol id=\"i-{}\" viewBox=\"0 0 {ICON_BOX} {ICON_BOX}\"><path d=\"{d}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1\" stroke-linejoin=\"round\"/></symbol>",
        e.info().short
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_but_junction_has_an_icon_that_is_path_data() {
        for e in ElementType::ALL {
            let Some(d) = icon(e) else {
                assert_eq!(e, ElementType::Junction, "{e:?} has no icon");
                continue;
            };
            assert!(!d.is_empty(), "{e:?}");
            for c in d.chars() {
                assert!(
                    "MLHVCSQTAZmlhvcsqtaz0123456789.,- ".contains(c),
                    "{e:?}: `{c}` is not path data"
                );
            }
            assert!(!d.contains("-0"), "{e:?}: negative zero would break byte stability");
        }
    }
}
