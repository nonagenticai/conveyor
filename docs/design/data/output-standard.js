window.CONVEYOR_OUTPUT_STANDARD = {
  "version": 1,
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
    "skills-assessment"
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
        "skills-assessment"
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
  ]
};
