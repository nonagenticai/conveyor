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
  -- pipeline runs attributed to the unit (PR run + merge run)
  ciRuns : Option Nat
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

def checkObs (o : UnitObs) : Bool :=
  oneBranch o && onePull o && pullUsable o && ciBudget o && mergeComplete o

def checkLedger (units : List UnitObs) : Bool :=
  !units.isEmpty && units.all checkObs

theorem merge_complete_implies (o : UnitObs) (h : mergeComplete o = true)
    (hm : o.pull = some .merged) :
    o.issue = some .closed ∧ o.branchDeleted = some true := by
  cases o with
  | mk issue branches branchDeleted pulls pull ciRuns =>
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
