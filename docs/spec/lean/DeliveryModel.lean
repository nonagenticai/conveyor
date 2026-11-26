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
