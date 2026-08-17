# ArchiMate 3.2 types, and what may connect to what

`amcli` embeds Archi's own relationship matrix, so the tool is the fastest way
to answer a legality question — you do not have to remember any of this.

    amcli relation add Serving "A" "B"      # a refusal names what IS permitted

## Element types by layer

**Strategy** Resource · Capability · CourseOfAction · ValueStream

**Business** BusinessActor · BusinessRole · BusinessCollaboration ·
BusinessInterface · BusinessProcess · BusinessFunction · BusinessInteraction ·
BusinessEvent · BusinessService · BusinessObject · Contract · Representation ·
Product

**Application** ApplicationComponent · ApplicationCollaboration ·
ApplicationInterface · ApplicationFunction · ApplicationInteraction ·
ApplicationProcess · ApplicationEvent · ApplicationService · DataObject

**Technology** Node · Device · SystemSoftware · TechnologyCollaboration ·
TechnologyInterface · Path · CommunicationNetwork · TechnologyFunction ·
TechnologyProcess · TechnologyInteraction · TechnologyEvent · TechnologyService ·
Artifact

**Physical** Equipment · Facility · DistributionNetwork · Material

**Motivation** Stakeholder · Driver · Assessment · Goal · Outcome · Principle ·
Requirement · Constraint · Meaning · Value

**Implementation & Migration** WorkPackage · Deliverable · ImplementationEvent ·
Plateau · Gap

**Other** Location · Grouping · Junction

## Relationship types

Composition · Aggregation · Assignment · Realization · Serving · Access ·
Influence · Triggering · Flow · Specialization · Association

Both the bare name and the suffixed form are accepted everywhere a type is named
— `relation add`, `-t`, `-r` and `type=` in a filter: `Serving` and
`ServingRelationship` mean the same thing. Output always prints the suffixed
form, because that is what the file says.

A name that is not a type is exit 2 with the types this model uses listed, so an
empty result always means "this model has none", never "you spelled it wrong".
`-t` takes one type; for a whole category use `kind=element` or `kind=relation`.

## Things that surprise people

- **A DataObject can only be Associated to an ApplicationComponent.** Not
  Serving, not Access. Access runs from the *function* to the data.
- **`--access write` is the default**, because accessType 0 means write in the
  schema. Use `--access read`, `--access rw` or `--access unspecified` for the
  others.
- **Composition and Aggregation between two DataObjects are permitted** by the
  standard, whatever a local modelling guideline may say. `amcli` enforces
  ArchiMate, not house style.
- **Every relationship touching a Junction must be the same type.**
- **Local convention is not the standard.** If your team forbids something
  ArchiMate allows, that belongs in a review, not in this tool.
