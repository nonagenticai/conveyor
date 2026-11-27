/-!
# The unit model's negative controls

`scripts/lean/check-model.sh controls` prepends `DeliveryModel.lean` verbatim (the same bytes every
certificate carries) and checks this file in the prover's own project. Every theorem below asserts
that a broken unit *satisfies* the rule, so the kernel must refuse every one: the file exits 1 with
exactly one `error:` per `theorem control_`. A model that cannot fail is not a model, and a control
that stops being refused fails the script's count.

This file is never embedded in a certificate.
-/

/-- The one unit every bad case is derived from: all eight rules hold. -/
def goodUnit : UnitObs :=
  { issue := some .closed, branches := some 1, branchDeleted := some true, pulls := some 1,
    pull := some .merged, ciRuns := some 2, closesKeyword := some true, verifyParts := some true,
    unit := some true }

theorem positive_control : checkUnit goodUnit = true := by decide

def twoBranches : UnitObs := { goodUnit with branches := some 2 }
theorem control_two_branches : checkUnit twoBranches = true := by decide

def twoPulls : UnitObs := { goodUnit with pulls := some 2 }
theorem control_two_pulls : checkUnit twoPulls = true := by decide

def draft : UnitObs := { goodUnit with pull := some .draft }
theorem control_draft : checkUnit draft = true := by decide

def threeCi : UnitObs := { goodUnit with ciRuns := some 3 }
theorem control_three_ci : checkUnit threeCi = true := by decide

def mergedIssuePending : UnitObs := { goodUnit with issue := some .pending }
theorem control_merged_issue_pending : checkUnit mergedIssuePending = true := by decide

def mergedBranchLive : UnitObs := { goodUnit with branchDeleted := some false }
theorem control_merged_branch_live : checkUnit mergedBranchLive = true := by decide

def unknownBranches : UnitObs := { goodUnit with branches := none }
theorem control_unknown_branches : checkUnit unknownBranches = true := by decide

def noCloses : UnitObs := { goodUnit with closesKeyword := some false }
theorem control_no_closes : checkUnit noCloses = true := by decide

def missingVerify : UnitObs := { goodUnit with verifyParts := none }
theorem control_missing_verify : checkUnit missingVerify = true := by decide

/-- A non-unit issue is outside the issue-content contract: `verifyParts` is vacuous there, so
a missing `Verify:` line cannot fail it. This theorem must COMPILE — it is the positive case. -/
def nonUnitNoParts : UnitObs := { goodUnit with unit := some false, verifyParts := none }
theorem positive_non_unit_parts_are_vacuous : checkUnit nonUnitNoParts = true := by decide

/-- An unreadable issue still fails closed: `none` is not `some false`, so it cannot take the
vacuous route and must satisfy `verifyParts` itself. -/
def unknownUnitNoParts : UnitObs := { goodUnit with unit := none, verifyParts := none }
theorem control_unknown_unit : checkUnit unknownUnitNoParts = true := by decide

/-- The empty ledger is refused by the aggregate, even though every rule of every unit is vacuous. -/
theorem control_empty_ledger : checkLedger [] = true := by decide

/-- A certificate that lies about one rule's outcome is refused: the kernel computes the truth. -/
theorem control_falsified_rules :
    ruleResults [goodUnit] =
      [("oneBranch", false), ("onePull", true), ("pullUsable", true), ("ciBudget", true),
       ("mergeComplete", true), ("closesKeyword", true), ("verifyParts", true)] := by
  decide
