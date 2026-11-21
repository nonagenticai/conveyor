window.CONVEYOR_OUTPUT_STANDARD = {
  "version": 2,
  "categories": [
    "project-brief",
    "inception-analysis",
    "architecture-design",
    "decision",
    "evidence",
    "closure-review",
    "vision",
    "blueprint",
    "business-case",
    "programme-plan",
    "governance",
    "reuse-map",
    "brief-template",
    "architecture-analysis",
    "spec",
    "benefit-profile",
    "benefits-map",
    "projects-dossier",
    "tranche-plan",
    "tranche-review",
    "benefits-strategy",
    "realization-plan",
    "stakeholder-profiles",
    "architecture-principles",
    "adm-profile",
    "artifact-standard",
    "architecture-artifact",
    "statement-of-architecture-work",
    "compliance-review",
    "waiver",
    "maturity-assessment",
    "skills-assessment",
    "pi-plan",
    "pi-objectives",
    "system-demo",
    "inspect-adapt"
  ],
  "aliases": {
    "brief": "project-brief",
    "analysis": "inception-analysis",
    "architecture": "architecture-design"
  },
  "classes": [
    {
      "name": "archived",
      "when": {
        "measured_status": "archived"
      },
      "now": [
        "project-brief"
      ],
      "closure": []
    },
    {
      "name": "p0",
      "when": {
        "key": "P0"
      },
      "now": [
        "vision",
        "blueprint",
        "business-case",
        "programme-plan",
        "governance",
        "reuse-map",
        "brief-template",
        "architecture-analysis",
        "benefit-profile",
        "benefits-map",
        "projects-dossier",
        "tranche-plan",
        "tranche-review",
        "benefits-strategy",
        "realization-plan",
        "stakeholder-profiles",
        "architecture-principles",
        "adm-profile",
        "artifact-standard",
        "maturity-assessment",
        "skills-assessment",
        "pi-plan",
        "pi-objectives",
        "system-demo",
        "inspect-adapt"
      ],
      "closure": []
    },
    {
      "name": "spec",
      "when": {
        "key": "SPEC-V2"
      },
      "now": [
        "spec"
      ],
      "closure": []
    },
    {
      "name": "t9",
      "when": {
        "key": "T9"
      },
      "now": [
        "project-brief",
        "inception-analysis",
        "architecture-design",
        "architecture-principles",
        "adm-profile",
        "artifact-standard",
        "architecture-artifact",
        "statement-of-architecture-work",
        "compliance-review",
        "waiver",
        "maturity-assessment",
        "skills-assessment"
      ],
      "closure": [
        "evidence",
        "closure-review"
      ]
    },
    {
      "name": "v2-proposed",
      "when": {
        "track": "v2",
        "measured_status": "proposed"
      },
      "now": [
        "project-brief",
        "inception-analysis",
        "architecture-design"
      ],
      "closure": [
        "evidence",
        "closure-review"
      ]
    },
    {
      "name": "v2-built",
      "when": {
        "track": "v2"
      },
      "now": [
        "project-brief",
        "evidence"
      ],
      "closure": [
        "closure-review"
      ]
    },
    {
      "name": "v1-active",
      "when": {
        "track": "v1",
        "measured_status": "partial"
      },
      "now": [
        "project-brief",
        "evidence"
      ],
      "closure": [
        "closure-review"
      ]
    },
    {
      "name": "v1-built",
      "when": {
        "track": "v1"
      },
      "now": [
        "project-brief",
        "evidence"
      ],
      "closure": []
    },
    {
      "name": "programme",
      "when": {
        "track": "programme"
      },
      "now": [
        "project-brief",
        "evidence"
      ],
      "closure": [
        "closure-review"
      ]
    }
  ],
  "templates": {
    "project-brief": {
      "required": [
        "Outcome",
        "Boundary",
        "Reuse sources",
        "Inception units",
        "Dependencies",
        "Acceptance",
        "Capabilities",
        "Plan"
      ],
      "optional": [
        "Risks",
        "Links"
      ],
      "skeleton": "# Project brief\n\nOne page: what this project changes, what it reuses, and how it is accepted.\n\n## Outcome\n\nOne sentence: what is true after this project that is not true now.\n\n## Boundary\n\nWhat is out of scope, and which existing surfaces this project must not fork.\n\n## Reuse sources\n\nExisting paths, estate services or upstream crates to analyse first; one line each.\n\n## Inception units\n\nThe analysis and architecture claims this project starts with.\n\n## Dependencies\n\nTyped: blocked-by, acceptance-blocked-by, blocks, related.\n\n## Acceptance\n\nA BDD scenario per part, each with its `Verify:` command.\n\n## Capabilities\n\nThe organisation positions or capabilities that own the outcome.\n\n## Plan\n\nThe slice plan path or the spec section this project is held to.\n\n## Risks\n\nWhat could stop the project, and the first check for each.\n\n## Links\n\nIssues, plans and evidence.\n"
    },
    "inception-analysis": {
      "required": [
        "Current state",
        "Reuse sources",
        "Build vs reuse",
        "Risks"
      ],
      "optional": [
        "Open questions",
        "Links"
      ],
      "skeleton": "# Inception analysis\n\nCurrent state and reuse for this project's capability: what already exists, what is refused, and what is claimed.\n\n## Current state\n\nThe code that exists today, citing `path:symbol` inside the repository; outside the repository name the capability, never a private host.\n\n## Reuse sources\n\nReuse inside the repository, in the wider estate, and upstream, each with the analysis question.\n\n## Build vs reuse\n\nThe decision per candidate, the boundary, and the licence or contract constraint.\n\n## Risks\n\nWhat breaks if this is built the obvious way, and the check that would reveal it.\n\n## Open questions\n\nQuestions the architecture must answer.\n\n## Links\n\nBrief, issues, coverage rows.\n"
    },
    "architecture-design": {
      "required": [
        "Context and boundary",
        "Aggregates, tables and actions",
        "Decisions",
        "Acceptance",
        "Architecture artefacts"
      ],
      "optional": [
        "Migration and rollout",
        "Links"
      ],
      "skeleton": "# Architecture design\n\nThis project's design to the architecture artefact standard: the to-be it adds to the blueprint.\n\n## Context and boundary\n\nWhat this design owns, what it must not duplicate, and the blueprint it fits.\n\n## Aggregates, tables and actions\n\nThe rows, invariants, actions and events; names must match the registry.\n\n## Decisions\n\nEach decision with its alternative and consequence, linked to its decision record.\n\n## Acceptance\n\nThe scenarios that prove the design, each with its `Verify:` command.\n\n## Architecture artefacts\n\nThe typed artefacts (catalogue, matrix or diagram) this phase and plateau require, with owner, reviewer and review date.\n\n## Migration and rollout\n\nOrder of deployment and the rollback.\n\n## Links\n\nContracts, brief, plan.\n"
    },
    "decision": {
      "required": [
        "Context",
        "Decision",
        "Consequences",
        "Evidence"
      ],
      "optional": [
        "Alternatives",
        "Links"
      ],
      "skeleton": "# Decision record\n\nOne decision, its context and its consequences, so the programme can be read without the conversation that made it.\n\n## Context\n\nThe situation, the constraints and what forced a choice.\n\n## Decision\n\nWhat was decided, in the active voice.\n\n## Consequences\n\nWhat becomes true, what becomes impossible, and what must be revisited.\n\n## Evidence\n\nThe measurement, spec section or artefact that supports the decision.\n\n## Alternatives\n\nThe options considered and why they lost.\n\n## Links\n\nThe artefacts and issues this decision governs.\n"
    },
    "evidence": {
      "required": [
        "Claim",
        "Evidence",
        "Verification",
        "Links",
        "Gap"
      ],
      "optional": [],
      "skeleton": "# Evidence\n\nWhat was claimed, what was measured, and what remains a gap.\n\n## Claim\n\nThe claim in one sentence.\n\n## Evidence\n\nThe measured result, with the command or path that produced it.\n\n## Verification\n\nHow an independent reader can repeat the check.\n\n## Links\n\nThe published page, contract, issue or artefact.\n\n## Gap\n\nWhat is not evidenced; `none` only when the claim is fully covered.\n"
    },
    "closure-review": {
      "required": [
        "Outcome vs acceptance",
        "Evidence",
        "Open items",
        "Decision"
      ],
      "optional": [],
      "skeleton": "# Closure review\n\nWhether a project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\nEach acceptance criterion, its evidence, and pass or fail.\n\n## Evidence\n\nThe measured artefacts: checks, pages, runs.\n\n## Open items\n\nWhat remains, with an owner and a next step; `none` when nothing does.\n\n## Decision\n\nClose, hold or return, and the reason.\n"
    },
    "vision": {
      "required": [
        "Purpose",
        "Outcomes",
        "Boundary",
        "Who it serves"
      ],
      "optional": [
        "Mandate",
        "Links"
      ],
      "skeleton": "# Vision and mandate\n\nWhy the programme exists, what it changes, and who it serves.\n\n## Purpose\n\nThe reason the programme exists, in one paragraph.\n\n## Outcomes\n\nThe outcomes the programme is judged on, not the capabilities it delivers.\n\n## Boundary\n\nWhat is in scope, what is explicitly out, and the licensing frame.\n\n## Who it serves\n\nThe stakeholders, their interest and the value each receives.\n\n## Mandate\n\nThe authority and its limits.\n\n## Links\n\nBlueprint, business case, programme plan.\n"
    },
    "blueprint": {
      "required": [
        "Aggregates and services",
        "Action registry and data model",
        "Deployment estate",
        "The to-be the projects deliver"
      ],
      "optional": [
        "Decisions",
        "Links"
      ],
      "skeleton": "# Blueprint\n\nThe target architecture and operating model the projects are held to.\n\n## Aggregates and services\n\nThe bounded contexts, their data ownership and the service boundaries.\n\n## Action registry and data model\n\nThe actions, tables, invariants and events that form the contract.\n\n## Deployment estate\n\nWhere each service runs and what it depends on.\n\n## The to-be the projects deliver\n\nEach project's contribution to this blueprint, and the gap it closes.\n\n## Decisions\n\nThe architecture decisions this blueprint rests on.\n\n## Links\n\nArtefacts, contracts, coverage.\n"
    },
    "business-case": {
      "required": [
        "Why now",
        "Benefit map",
        "Cost and risk posture",
        "Licensing frame"
      ],
      "optional": [
        "Options",
        "Decision"
      ],
      "skeleton": "# Business case\n\nWhy now, what it buys, what it costs and the risk posture.\n\n## Why now\n\nThe problem, the cost of doing nothing, and the trigger for action.\n\n## Benefit map\n\nEach benefit, its mechanism and its measure; a benefit with no measure is a wish.\n\n## Cost and risk posture\n\nThe engineering cost, the recurring cost, and the exposure accepted.\n\n## Licensing frame\n\nThe public, opt-in and tenant boundary of the investment.\n\n## Options\n\nThe options considered including doing nothing.\n\n## Decision\n\nThe investment decision and its authority.\n"
    },
    "programme-plan": {
      "required": [
        "Tranches",
        "Projects and sequencing",
        "Gates",
        "First three months"
      ],
      "optional": [
        "Baseline",
        "Links"
      ],
      "skeleton": "# Programme plan\n\nThe projects, their sequence and the gates between them.\n\n## Tranches\n\nEach tranche, its theme and its review point.\n\n## Projects and sequencing\n\nEvery project, its tranche, its dependencies and its deliverable.\n\n## Gates\n\nThe gate that lets each tranche start and what closes it.\n\n## First three months\n\nThe first units, their order and the evidence each must produce.\n\n## Baseline\n\nThe plan this one is measured against.\n\n## Links\n\nThe seed and the coverage instrument.\n"
    },
    "governance": {
      "required": [
        "Roles",
        "Decision ladder",
        "Assurance",
        "The unit protocol"
      ],
      "optional": [
        "Board",
        "Links"
      ],
      "skeleton": "# Governance and organisation\n\nThe roles, the decision rights and the assurance that make the programme accountable.\n\n## Roles\n\nThe positions, who holds each, and their decision limits.\n\n## Decision ladder\n\nThe rule, the automated layer, the generation and the human decision that closes it.\n\n## Assurance\n\nWho reviews what, on which evidence, and how refusal is recorded.\n\n## The unit protocol\n\nHow a unit is claimed, built, verified and merged.\n\n## Board\n\nThe architecture and programme boards, their charter and their limits.\n\n## Links\n\nThe organisation seed and the authority rows.\n"
    },
    "reuse-map": {
      "required": [
        "Reuse candidates",
        "Boundary",
        "Licence and contract constraints"
      ],
      "optional": [
        "Links"
      ],
      "skeleton": "# Reuse map\n\nWhat exists that can be reused, what it costs to reuse, and where the boundary is.\n\n## Reuse candidates\n\nOne row per candidate: source, what to reuse, boundary, licence or contract constraint.\n\n## Boundary\n\nWhat must not be reused, and the reason.\n\n## Licence and contract constraints\n\nThe terms that decide whether reuse is allowed.\n\n## Links\n\nThe repository, estate services, upstream crates.\n"
    },
    "brief-template": {
      "required": [
        "Mandatory fields",
        "Inception units",
        "Plan"
      ],
      "optional": [],
      "skeleton": "# Brief template\n\nThe mandatory shape of a delivery brief, so every project is described the same way.\n\n## Mandatory fields\n\nOutcome, boundary, reuse sources, inception units, dependencies, acceptance, capabilities, plan.\n\n## Inception units\n\nThe analysis and architecture pair every project starts with.\n\n## Plan\n\nWhere the project's plan lives and how it is kept current.\n"
    },
    "architecture-analysis": {
      "required": [
        "Current state",
        "Options considered",
        "Risks",
        "Dependency map"
      ],
      "optional": [
        "Decisions",
        "Artefacts"
      ],
      "skeleton": "# Architecture and analysis pack\n\nThe current state, the options, the risks and the dependency map the blueprint rests on.\n\n## Current state\n\nWhat exists today, measured and cited.\n\n## Options considered\n\nThe build, reuse and buy options, with their trade-offs.\n\n## Risks\n\nThe risks that shape the architecture, with the check for each.\n\n## Dependency map\n\nThe project dependencies and the order they force.\n\n## Decisions\n\nThe decisions this pack records.\n\n## Artefacts\n\nThe typed artefacts that support it.\n"
    },
    "spec": {
      "required": [
        "Context",
        "Requirements",
        "Design",
        "Acceptance"
      ],
      "optional": [],
      "skeleton": "# Specification\n\nA binding specification: what must be true, not how it is built.\n\n## Context\n\nThe problem and the boundary of this specification.\n\n## Requirements\n\nEach requirement is testable; name the condition, constraint or delta.\n\n## Design\n\nThe chosen design and the decisions that constrain it.\n\n## Acceptance\n\nThe scenarios that prove the requirements, each with its `Verify:` command.\n"
    },
    "benefit-profile": {
      "required": [
        "Identification and purpose",
        "Performance",
        "Cost",
        "Conditions",
        "Uncertainty and dependencies",
        "Responsibility and attribution"
      ],
      "optional": [],
      "skeleton": "# Benefit profile\n\nOne benefit, measured, owned and attributed. A profile missing a baseline, target, method or owner is refused.\n\n## Identification and purpose\n\nReference, description, category, objectives and the observable outcome.\n\n## Performance\n\nAffected KPIs, the baseline (or the literal `not measured`), the target, the measurement method, the financial value where expressible.\n\n## Cost\n\nThe cost of realizing the benefit, including the business change it needs.\n\n## Conditions\n\nCapabilities required, outcomes that must already exist, and the operating-practice change.\n\n## Uncertainty and dependencies\n\nRelated risks and issues, and dependencies outside the programme.\n\n## Responsibility and attribution\n\nWho realizes it, the BCM owner, and the operational area the improvement is attributed to.\n"
    },
    "benefits-map": {
      "required": [
        "Benefit paths",
        "Dependencies",
        "Benefits at risk"
      ],
      "optional": [],
      "skeleton": "# Benefits map\n\nEvery path from a project output to a strategic objective, and the benefits put at risk when a project slips.\n\n## Benefit paths\n\nEach path: output → capability → operational outcome → benefit or disbenefit → strategic objective.\n\n## Dependencies\n\nThe dependencies between benefits and on project outputs.\n\n## Benefits at risk\n\nWhich benefits a slipping project or capability puts at risk, and the early warning.\n"
    },
    "projects-dossier": {
      "required": [
        "Projects",
        "Tranches and dependencies",
        "Resources and budget",
        "Delivery risks"
      ],
      "optional": [],
      "skeleton": "# Projects dossier\n\nEvery project, its outputs, resources, dependencies and contribution to the blueprint.\n\n## Projects\n\nOne row per project: key, output, tranche, status and plan.\n\n## Tranches and dependencies\n\nThe tranche each project sits in and its typed dependencies.\n\n## Resources and budget\n\nThe resource assumption and the budget posture; an unknown is stated as unknown.\n\n## Delivery risks\n\nThe risks affecting delivery and the first check for each.\n"
    },
    "tranche-plan": {
      "required": [
        "Tranches",
        "Entry and exit criteria",
        "Review dates",
        "Sequencing"
      ],
      "optional": [],
      "skeleton": "# Tranche plan\n\nThe tranche sequence, its criteria and its review dates. It adopts the programme seed; it never invents a parallel list.\n\n## Tranches\n\nNumber, theme and the projects each contains.\n\n## Entry and exit criteria\n\nWhat must be true to start and to close each tranche.\n\n## Review dates\n\nThe end-of-tranche review date and the evidence it will judge.\n\n## Sequencing\n\nWhy this order, and which dependency forces it.\n"
    },
    "tranche-review": {
      "required": [
        "Evidence",
        "Benefit status",
        "Risk and issue register",
        "Decision"
      ],
      "optional": [],
      "skeleton": "# Tranche review\n\nThe end-of-tranche approval: continue, pause or stop, on evidence. A review with no evidence is refused.\n\n## Evidence\n\nThe baselines, the benefit status and the risk and issue register the review judged.\n\n## Benefit status\n\nEach benefit against its profile: measured, on track, at risk or unmeasured.\n\n## Risk and issue register\n\nThe open risks and issues, with owners.\n\n## Decision\n\nContinue, pause or stop, with the reason and the conditions.\n"
    },
    "benefits-strategy": {
      "required": [
        "Measurement methods",
        "Review cycle",
        "Roles and assessment",
        "Realization process"
      ],
      "optional": [],
      "skeleton": "# Benefits management strategy\n\nHow benefits are measured, reviewed and judged, and who owns each step.\n\n## Measurement methods\n\nThe method and source for each KPI; a method that does not exist yet is named as a gap.\n\n## Review cycle\n\nWhen benefits are reviewed, at which gate, and against which baseline.\n\n## Roles and assessment\n\nThe BCM, the SRO and the assessor for each review.\n\n## Realization process\n\nBaseline, monitor, transition, readiness, enact, measure, retire and keep measuring.\n"
    },
    "realization-plan": {
      "required": [
        "Transition",
        "Embedding",
        "Post-closure measurement",
        "Ownership"
      ],
      "optional": [],
      "skeleton": "# Benefits realization plan\n\nThe transition, the embedding, and who keeps measuring after the programme closes.\n\n## Transition\n\nEach benefit's transition entry, its readiness check and its handover.\n\n## Embedding\n\nThe operating-practice changes and the activities that embed them.\n\n## Post-closure measurement\n\nWho measures after closure, with which method and which cadence.\n\n## Ownership\n\nThe benefit owner, the operational owner and the date each takes over.\n"
    },
    "stakeholder-profiles": {
      "required": [
        "Stakeholders",
        "Concerns and sensitivities",
        "Influence and interest",
        "Support and engagement",
        "Benefits and disbenefits"
      ],
      "optional": [],
      "skeleton": "# Stakeholder profiles\n\nWho the programme serves, what they care about, and how each group is engaged.\n\n## Stakeholders\n\nEach group or position with its role in the programme.\n\n## Concerns and sensitivities\n\nWhat each group cares about and what would lose its support.\n\n## Influence and interest\n\nImportance, influence, interest and exposure, each rated.\n\n## Support and engagement\n\nSupport level and trend, the engagement target, and the engagement action.\n\n## Benefits and disbenefits\n\nThe benefits and disbenefits distributed to each group.\n"
    },
    "architecture-principles": {
      "required": [
        "Principles",
        "Approval",
        "Application"
      ],
      "optional": [],
      "skeleton": "# Architecture principles\n\nThe principles the Architecture Board applies, each with its rationale and implications.\n\n## Principles\n\nEach principle: statement, rationale and implications; a principle without implications is a slogan.\n\n## Approval\n\nThe board, the date and the decision that adopted the catalogue.\n\n## Application\n\nHow a deviation is detected, and the waiver or decision that handles it.\n"
    },
    "adm-profile": {
      "required": [
        "Phases",
        "Artefacts by phase and plateau",
        "Contracts",
        "Adoption"
      ],
      "optional": [],
      "skeleton": "# Tailored ADM profile\n\nWhich ADM phases run, which artefacts each phase and plateau requires, and which contract closes it.\n\n## Phases\n\nThe phases that run for this organisation and the order they run in.\n\n## Artefacts by phase and plateau\n\nThe mandatory artefacts per phase and per plateau: project start, high-level design and solution design.\n\n## Contracts\n\nThe contract kind that closes each phase and who approves it.\n\n## Adoption\n\nThe board decision that adopted this profile and the review date.\n"
    },
    "artifact-standard": {
      "required": [
        "Common metadata",
        "Catalogues",
        "Matrices",
        "Diagrams",
        "Links",
        "Repository"
      ],
      "optional": [],
      "skeleton": "# Artefact standard\n\nThe typed artefact standard the programme adopts: metadata, the three types, the plateaus and the links.\n\n## Common metadata\n\nTitle, status, version, author and date, reviewer and review date, related initiative, rationale and the document or contract it sits under.\n\n## Catalogues\n\nThe unique identifier, item name and description of use or structure.\n\n## Matrices\n\nThe two axes and the relationship the matrix records.\n\n## Diagrams\n\nThe building block or metamodel and the viewpoint.\n\n## Links\n\nDerivation, realization, reuse and dependency edges between artefacts.\n\n## Repository\n\nWhere artefacts live and how the standard is adopted and versioned.\n"
    },
    "architecture-artifact": {
      "required": [
        "Metadata",
        "Entries",
        "Links",
        "Document"
      ],
      "optional": [],
      "skeleton": "# Architecture artefact\n\nOne typed artefact: a catalogue, a matrix or a diagram, with its metadata, entries and links.\n\n## Metadata\n\nType, title, version, owner, reviewer and review date, phase, plateau and the related project.\n\n## Entries\n\nThe rows: the item and its description, the two axes and their value, or the element and its viewpoint.\n\n## Links\n\nWhat this artefact derives from, realizes, reuses or depends on.\n\n## Document\n\nThe published document or contract this artefact sits under.\n"
    },
    "statement-of-architecture-work": {
      "required": [
        "Assignment",
        "Scope",
        "Responsibilities",
        "Acceptance criteria",
        "Contract"
      ],
      "optional": [],
      "skeleton": "# Statement of Architecture Work\n\nThe Phase A contract: the assignment, its scope and the criteria by which the architecture is accepted.\n\n## Assignment\n\nWhat the architecture work is asked to deliver and for whom.\n\n## Scope\n\nThe boundaries, the artefacts expected and the phase set.\n\n## Responsibilities\n\nWho does the work and who accepts it.\n\n## Acceptance criteria\n\nThe conditions the architecture must meet to close Phase A.\n\n## Contract\n\nThe contract record, its version and its approval.\n"
    },
    "compliance-review": {
      "required": [
        "Relationship",
        "Findings",
        "Waivers",
        "Decision"
      ],
      "optional": [],
      "skeleton": "# Compliance review\n\nThe Phase G review of one delivery against its governing architecture contract.\n\n## Relationship\n\nExactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.\n\n## Findings\n\nThe evidence for each finding, against the contract.\n\n## Waivers\n\nEach accepted deviation with its owner, reason, cost, risk and reassessment date.\n\n## Decision\n\nThe outcome and the conditions that attach to it.\n"
    },
    "waiver": {
      "required": [
        "Deviation",
        "Accepting owner",
        "Reason and impact",
        "Reassessment"
      ],
      "optional": [],
      "skeleton": "# Architecture waiver\n\nAn accepted deviation from the architecture contract, with an owner and a date to revisit it.\n\n## Deviation\n\nWhat deviates from which contract clause.\n\n## Accepting owner\n\nThe asset owner who accepts the deviation and the scope of their authority.\n\n## Reason and impact\n\nWhy the deviation is needed, the additional cost and the risk.\n\n## Reassessment\n\nThe reassessment date and the condition that ends the waiver.\n"
    },
    "maturity-assessment": {
      "required": [
        "Dimensions",
        "Scores",
        "Baseline",
        "Milestones"
      ],
      "optional": [],
      "skeleton": "# Architecture maturity assessment\n\nThe nine-dimension maturity baseline, scored 0–5 with evidence and a development plan.\n\n## Dimensions\n\nArchitecture process, architecture development, business linkage, senior-management involvement, operating-unit participation, architecture communication, IT security, architecture governance, and IT investment and acquisition strategy.\n\n## Scores\n\nEach dimension's score from 0 (none) to 5 (measured), with the evidence for the rating.\n\n## Baseline\n\nThe dated baseline the next assessment is compared with.\n\n## Milestones\n\nThe development milestones a tranche review can hold the capability to.\n"
    },
    "skills-assessment": {
      "required": [
        "Categories",
        "Proficiency by role",
        "Baseline",
        "Development milestones"
      ],
      "optional": [],
      "skeleton": "# Architecture skills assessment\n\nThe seven-category skills baseline per role, with proficiency and development milestones.\n\n## Categories\n\nGeneral; business skills and methods; architecture skills; programme and project management; general IT knowledge; technical IT skills; legal environment.\n\n## Proficiency by role\n\nEach role's proficiency per category, with the evidence.\n\n## Baseline\n\nThe dated baseline and its scope.\n\n## Development milestones\n\nThe development plan and the milestones a review can measure.\n"
    },
    "pi-plan": {
      "required": [
        "Objectives",
        "Plan",
        "WSJF order",
        "Capacity and WIP",
        "Evidence"
      ],
      "optional": [
        "Dependencies",
        "Links"
      ],
      "skeleton": "# PI plan\n\nThe plan for one programme increment: the deliverables selected from the backlog, ordered by WSJF, within the WIP limit.\n\n## Objectives\n\nThe PI objectives this plan is judged against, with their planned business value.\n\n## Plan\n\nThe deliverables materialised for this increment, one per WBS node, with their project and acceptance.\n\n## WSJF order\n\nThe four typed inputs per candidate — business value, time criticality, risk reduction and job size — in the order the plan takes them. The score is computed, never stored.\n\n## Capacity and WIP\n\nThe capacity the plan is bounded by and the WIP limit it respects.\n\n## Evidence\n\nThe backlog rows, the cadence row and the approval that made this the plan.\n\n## Dependencies\n\nThe cross-project dependencies the order is constrained by.\n\n## Links\n\nThe cadence, the objectives and the tranche this plan belongs to.\n"
    },
    "pi-objectives": {
      "required": [
        "Objectives",
        "Business value",
        "Measurement",
        "Evidence"
      ],
      "optional": [
        "Risks",
        "Links"
      ],
      "skeleton": "# PI objectives\n\nThe objectives for one programme increment, planned before delivery and assessed at its close.\n\n## Objectives\n\nEach objective, in outcome form: what is true after the increment that is not true now.\n\n## Business value\n\nThe planned business value per objective, typed as an integer; the actual value is recorded at the close.\n\n## Measurement\n\nHow each objective is judged: the acceptance manifest, the evidence and the check that decides it.\n\n## Evidence\n\nThe verification runs and the records that support the assessment.\n\n## Risks\n\nThe objectives at risk and what would move them.\n\n## Links\n\nThe PI plan, the tranche review and the project records.\n"
    },
    "system-demo": {
      "required": [
        "Demonstrated",
        "Evidence",
        "Credits",
        "Findings"
      ],
      "optional": [
        "Links"
      ],
      "skeleton": "# System demo\n\nThe demo record for one programme increment: every completed deliverable exhibits its evidence. No demo, no credit.\n\n## Demonstrated\n\nEach completed deliverable presented, with its project and the outcome it evidences.\n\n## Evidence\n\nThe evidence each deliverable exhibited: the acceptance manifest run, the published output and the record.\n\n## Credits\n\nThe demo credits recorded per accepted deliverable, copied from the deliverable's evidence.\n\n## Findings\n\nWhat the demo exposed: defects, gaps and follow-ups, with owners.\n\n## Links\n\nThe cadence row, the deliverables and the tranche review.\n"
    },
    "inspect-adapt": {
      "required": [
        "What went well",
        "What did not",
        "Lessons",
        "Actions"
      ],
      "optional": [
        "Links"
      ],
      "skeleton": "# Inspect and adapt\n\nThe retro record for one programme increment: what the evidence showed, the lessons it produced and the actions it set.\n\n## What went well\n\nThe practices and decisions the evidence supports keeping.\n\n## What did not\n\nThe failures, delays and bypasses the evidence exposed, named plainly.\n\n## Lessons\n\nOne lesson per finding, as the organisational-memory record; a retro with no lessons is refused.\n\n## Actions\n\nThe actions, owners and dates that follow from the lessons.\n\n## Links\n\nThe retro cadence, the lessons and the tranche review.\n"
    }
  }
};
