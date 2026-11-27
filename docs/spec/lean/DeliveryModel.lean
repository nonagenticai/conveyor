/-!
# conveyor delivery model — the hand-written half of every certificate

This file is embedded verbatim at the head of every rendered certificate
(`crates/actions/src/proof/render.rs`), so it must stay small and elaborate fast.
It is core Lean only: no Mathlib import and no compiler-native reduction. Every
theorem is closed by the kernel (`decide` and `simp`).

It models **§5.3–§5.4 of `docs/SPEC_V2.md`**: plan ≠ evidence, a gate carries evidence,
and a closed item is terminal. It does *not* model the Rust engine that produces the facts —
the refinement from implementation to this model is assumed, not proven, and every
certificate states that assumption (see `README.md` in this directory).
-/

inductive State where
  | planned
  | ready
  | inProgress
  | blocked
  | closed
  deriving DecidableEq, BEq, Repr

def step : State → Option State
  | .planned => some .ready
  | .ready => some .inProgress
  | .inProgress => some .blocked
  | .blocked => some .inProgress
  | .closed => none

structure Path where
  states : List State
  evidence : List String
  deriving DecidableEq, BEq, Repr

def endsClosed (p : Path) : Bool :=
  p.states.getLast? == some .closed

def accepted (p : Path) : Bool :=
  endsClosed p && !p.evidence.isEmpty

structure SliceFacts where
  specSha256 : String
  modelSha256 : String
  state : State
  required : List String
  present : List String
  deriving DecidableEq, BEq, Repr

def covered (required present : List String) : Bool :=
  required.all (fun r => present.contains r)

def checkSlice (f : SliceFacts) : Bool :=
  (f.state == .closed) && covered f.required f.present

def noDuplicates (xs : List String) : Bool :=
  xs.eraseDups.length == xs.length

def sameSet (a b : List String) : Bool :=
  noDuplicates a && noDuplicates b && a.all (fun x => b.contains x) && b.all (fun x => a.contains x)

structure ProgrammeFacts where
  specSha256 : String
  modelSha256 : String
  expected : List String
  leaves : List (String × String)
  deriving DecidableEq, BEq, Repr

def leafNames (leaves : List (String × String)) : List String :=
  leaves.map (fun leaf => leaf.1)

def leafDigests (leaves : List (String × String)) : List String :=
  leaves.map (fun leaf => leaf.2)

def checkProgramme (f : ProgrammeFacts) : Bool :=
  sameSet f.expected (leafNames f.leaves) && noDuplicates (leafDigests f.leaves)

structure PackageFacts where
  specSha256 : String
  modelSha256 : String
  migrations : List String
  ledger : List String
  routes : List String
  served : List String
  checks : List (String × Bool)
  deriving DecidableEq, BEq, Repr

def checkPackage (f : PackageFacts) : Bool :=
  sameSet f.migrations f.ledger &&
    f.routes.all (fun r => f.served.contains r) &&
    f.checks.all (fun c => c.2)

theorem closed_is_terminal : step .closed = none := by decide

theorem no_self_transition : ∀ s t, step s = some t → t ≠ s := by
  intro s t h
  cases s <;> simp [step] at h <;> subst h <;> decide

theorem accepted_path_carries_evidence : ∀ p, accepted p = true → p.evidence ≠ [] := by
  intro p h
  cases h' : p.evidence with
  | nil => simp [accepted, endsClosed, h'] at h
  | cons x xs => simp

/-! ## Delivery units — `docs/plans/03-DELIVERY.md`

One issue, one branch, one non-draft PR, at most two CI runs, merged, the issue closed and
the branch deleted. Every fact is an observation; `none` means "not observed" and fails
closed. These are the GitHub-observable facts, collected with provenance — not the engine
rows, which do not carry issue state or branch liveness. -/

inductive IssueState where
  | pending
  | closed
  deriving DecidableEq, BEq, Repr

inductive PrState where
  | draft
  | ready
  | merged
  deriving DecidableEq, BEq, Repr

structure UnitObs where
  issue : Option IssueState
  -- distinct claim-branch names: live `issue/<n>-*` refs ∪ PR head refs
  branches : Option Nat
  -- the PR head ref is not live
  branchDeleted : Option Bool
  pulls : Option Nat
  pull : Option PrState
  -- session CI attempts: `ci`-workflow pull_request runs on the PR head. The queue and
  -- post-merge runs are the merge mechanics, reported beside the certificate, never inputs.
  ciRuns : Option Nat
  -- the linked PR's body names this issue (`Closes|Fixes|Resolves #N`)
  closesKeyword : Option Bool
  -- every `### P` part of the issue body carries a `Verify:` line
  verifyParts : Option Bool
  deriving DecidableEq, BEq, Repr

def oneBranch (o : UnitObs) : Bool :=
  match o.branches with
  | some 1 => true
  | _ => false

def onePull (o : UnitObs) : Bool :=
  match o.pulls with
  | some 1 => true
  | _ => false

def pullUsable (o : UnitObs) : Bool :=
  match o.pull with
  | some .ready | some .merged => true
  | _ => false

def ciBudget (o : UnitObs) : Bool :=
  match o.ciRuns with
  | some n => decide (n ≤ 2)
  | none => false

def mergeComplete (o : UnitObs) : Bool :=
  match o.pull, o.issue, o.branchDeleted with
  | some .merged, some .closed, some true => true
  | some .merged, _, _ => false
  | _, _, _ => true

def closesStated (o : UnitObs) : Bool :=
  o.closesKeyword == some true

def partsVerified (o : UnitObs) : Bool :=
  o.verifyParts == some true

def checkObs (o : UnitObs) : Bool :=
  oneBranch o && onePull o && pullUsable o && ciBudget o && mergeComplete o &&
    closesStated o && partsVerified o

/-- The per-unit rule under the name the delivery contract uses. -/
def checkUnit (o : UnitObs) : Bool :=
  checkObs o

def checkLedger (units : List UnitObs) : Bool :=
  !units.isEmpty && units.all checkObs

/-- The per-rule outcome over a ledger. A certificate names this list as a literal, so the
kernel proves the reported outcome of every rule, not just the aggregate. -/
def ruleResults (units : List UnitObs) : List (String × Bool) :=
  [("oneBranch", units.all oneBranch),
   ("onePull", units.all onePull),
   ("pullUsable", units.all pullUsable),
   ("ciBudget", units.all ciBudget),
   ("mergeComplete", units.all mergeComplete),
   ("closesKeyword", units.all closesStated),
   ("verifyParts", units.all partsVerified)]

theorem merge_complete_implies (o : UnitObs) (h : mergeComplete o = true)
    (hm : o.pull = some .merged) :
    o.issue = some .closed ∧ o.branchDeleted = some true := by
  cases o with
  | mk issue branches branchDeleted pulls pull ciRuns closesKeyword verifyParts =>
    cases hm
    cases issue with
    | none => simp [mergeComplete] at h
    | some i =>
      cases i with
      | pending => simp [mergeComplete] at h
      | closed =>
        cases branchDeleted with
        | none => simp [mergeComplete] at h
        | some b =>
          cases b with
          | false => simp [mergeComplete] at h
          | true => simp

/-! ## The engine's recorded unit — `checkRecord`

`UnitObs` above is the GitHub-observed contract, collected outside the engine. The engine's own
unit record is a different subject: a `development_project` charter plus its recorded steps and
gate decisions. `proofs.attest` derives these facts from rows only, and `checkRecord` is what the
unit's certificate theorem closes. Every field is `Option`, so an unobserved fact is `none` and
fails closed — the engine never defaults to a pass. -/

structure UnitRecordFacts where
  specSha256 : String
  modelSha256 : String
  issueRef : Option String
  repo : Option String
  lane : Option String
  pullNumber : Option Nat
  pullUrl : Option String
  pullHead : Option String
  ciRunId : Option String
  ciSha : Option String
  ciVerdict : Option String
  mergeSha : Option String
  deployDigest : Option String
  servingDigest : Option String
  liveHealth : Option Nat
  liveAcceptanceExit : Option Int
  gateReview : Option Bool
  gateMerge : Option Bool
  gateDeploy : Option Bool
  deriving DecidableEq, BEq, Repr

def nonEmpty (value : Option String) : Bool :=
  match value with
  | some text => !text.isEmpty
  | none => false

def positive (value : Option Nat) : Bool :=
  match value with
  | some number => decide (0 < number)
  | none => false

/-- The recorded unit is complete: a charter with its issue, a pull request, a green CI run on
that head, a merge, a deploy, a live check at the deployed digest, and three approved gates. The
live digest must equal the deployed digest — a live check that ran elsewhere evidences nothing. -/
def checkRecord (f : UnitRecordFacts) : Bool :=
  nonEmpty f.issueRef && nonEmpty f.repo && nonEmpty f.lane &&
    positive f.pullNumber && nonEmpty f.pullUrl && nonEmpty f.pullHead &&
    nonEmpty f.ciRunId && nonEmpty f.ciSha && f.ciVerdict == some "green" &&
    nonEmpty f.mergeSha && nonEmpty f.deployDigest && nonEmpty f.servingDigest &&
    f.liveHealth == some 200 && f.liveAcceptanceExit == some 0 &&
    f.gateReview == some true && f.gateMerge == some true && f.gateDeploy == some true &&
    f.servingDigest == f.deployDigest

/-- A record with no merge cannot be proven. This is the fail-closed property in the kernel, not
in the renderer: an absent fact can never be rendered into a passing certificate. -/
theorem record_without_merge_is_unproven (f : UnitRecordFacts) (h : f.mergeSha = none) :
    checkRecord f = false := by
  simp [checkRecord, nonEmpty, h]

