import Lean

open Lean

/- A second, explicitly scoped model for the production-shaped publication
   domain. Natural-number IDs abstract validated UUIDs; counters abstract
   positive u32 inputs. Digests/authentication are established before this model.
   Evidence, proposals, relations and durable storage are NOT modeled here. -/
namespace MemoryFederation.Publication

inductive Kind where
  | grant | revoke | memory | state
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, FromJson

structure Command where
  sequence : Nat
  id : Nat
  publisher : Nat
  authenticated : Nat
  kind : Kind
  subject : Nat := 0
  origin : Nat := 0
  canonical : Nat := 0
  shared : Nat := 0
  «local» : Nat := 0
  revision : Nat := 0
  author : String := "expert"
  text : String := "knowledge"
  tombstone : Bool := false
  deriving Repr, DecidableEq, BEq

-- Deriving FromJson does not apply structure defaults to missing Nat/String
-- fields. Match the Rust fixture reader explicitly; malformed present fields
-- still fail instead of being replaced by defaults.
private def fieldDefault [FromJson α] (j : Json) (key : String) (fallback : α) : Except String α :=
  match j.getObjVal? key with
  | .ok value => fromJson? value
  | .error _ => pure fallback

instance : FromJson Command where
  fromJson? j := do
    return {
      sequence := ← j.getObjValAs? Nat "sequence"
      id := ← j.getObjValAs? Nat "id"
      publisher := ← j.getObjValAs? Nat "publisher"
      authenticated := ← j.getObjValAs? Nat "authenticated"
      kind := ← j.getObjValAs? Kind "kind"
      subject := ← fieldDefault j "subject" 0
      origin := ← fieldDefault j "origin" 0
      canonical := ← fieldDefault j "canonical" 0
      shared := ← fieldDefault j "shared" 0
      «local» := ← fieldDefault j "local" 0
      revision := ← fieldDefault j "revision" 0
      author := ← fieldDefault j "author" "expert"
      text := ← fieldDefault j "text" "knowledge"
      tombstone := ← fieldDefault j "tombstone" false
    }

structure State where
  sequence : Nat := 0
  publishers : List Nat := [1]
  -- Newest first; every accepted command is kept verbatim.
  accepted : List Command := []
  deriving Repr, DecidableEq, BEq, Inhabited

def latest (s : State) (kind : Kind) (origin canonical : Nat) : Option Command :=
  s.accepted.find? fun r => r.kind == kind && r.origin == origin && r.canonical == canonical

def contentVersion (s : State) (r : Command) : Nat :=
  ((latest s .memory r.origin r.canonical).map (·.shared)).getD 0

def stateRevision (s : State) (r : Command) : Nat :=
  ((latest s .state r.origin r.canonical).map (·.revision)).getD 0

def known (s : State) (r : Command) : Option Command :=
  s.accepted.find? fun old => old.publisher == r.publisher && old.id == r.id

def counterDecision (current actual : Nat) (gap : String) : String :=
  if current ≥ 4294967295 then "counter_exhausted"
  else if actual > current + 1 then gap
  else if actual < current + 1 then "counter_conflict"
  else "applied"

def memoryDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if r.publisher ≠ r.origin then "not_origin"
  else counterDecision (contentVersion s r) r.shared "version_gap"

def stateDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if r.publisher ≠ r.origin then "not_origin"
  else if contentVersion s r = 0 ∨ r.shared > contentVersion s r then "missing_dependency"
  else if r.shared < contentVersion s r then "counter_conflict"
  else counterDecision (stateRevision s r) r.revision "revision_gap"

def decision (s : State) (r : Command) : String :=
  if r.authenticated ≠ r.publisher then "forged_publisher"
  else match known s r with
  | some old => if old = r then "duplicate" else "equivocation"
  | none =>
    if s.sequence ≥ 4294967295 then "counter_exhausted"
    else if r.sequence > s.sequence + 1 then "order_gap"
    else if r.sequence < s.sequence + 1 then "stale_order"
    else match r.kind with
    | .grant | .revoke => if r.publisher = 1 then "applied" else "unauthorized"
    | .memory => memoryDecision s r
    | .state => stateDecision s r

def accept (s : State) (r : Command) : State :=
  { sequence := r.sequence, accepted := r :: s.accepted,
    publishers := match r.kind with
      | .grant => r.subject :: s.publishers.filter (· != r.subject)
      | .revoke => s.publishers.filter (· != r.subject)
      | _ => s.publishers }

def apply (s : State) (r : Command) : State × String :=
  let outcome := decision s r
  (if outcome = "applied" then accept s r else s, outcome)

theorem rejected_unchanged (s : State) (r : Command)
    (h : (apply s r).2 ≠ "applied") : (apply s r).1 = s := by
  have outcome : decision s r ≠ "applied" := h
  simp [apply, outcome]

theorem duplicate_unchanged (s : State) (r : Command)
    (identity : r.authenticated = r.publisher) (seen : known s r = some r) :
    apply s r = (s, "duplicate") := by
  simp [apply, decision, identity, seen]

theorem accepted_retry_idempotent (s : State) (r : Command)
    (identity : r.authenticated = r.publisher) (h : (apply s r).2 = "applied") :
    apply (apply s r).1 r = ((apply s r).1, "duplicate") := by
  have outcome : decision s r = "applied" := h
  have seen : known (apply s r).1 r = some r := by
    simp [known, apply, outcome, accept]
  exact duplicate_unchanged _ r identity seen

theorem accepted_preserves_command (s : State) (r : Command)
    (h : (apply s r).2 = "applied") :
    (apply s r).1.accepted = r :: s.accepted := by
  have outcome : decision s r = "applied" := h
  simp [apply, outcome, accept]

theorem counter_accepted_next (current actual : Nat) (gap : String)
    (notApplied : gap ≠ "applied")
    (h : counterDecision current actual gap = "applied") : actual = current + 1 := by
  unfold counterDecision at h
  split at h
  · contradiction
  · split at h
    · exact False.elim (notApplied h)
    · split at h
      · contradiction
      · omega

theorem memory_accepted_authorized_and_next (s : State) (r : Command)
    (h : memoryDecision s r = "applied") :
    r.publisher ∈ s.publishers ∧ r.publisher = r.origin ∧
      r.shared = contentVersion s r + 1 := by
  unfold memoryDecision at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · exact ⟨by simpa using ‹¬¬ r.publisher ∈ s.publishers›, by omega,
        counter_accepted_next _ _ _ (by decide) h⟩

theorem applied_memory_decision (s : State) (r : Command)
    (kind : r.kind = .memory) (h : (apply s r).2 = "applied") :
    memoryDecision s r = "applied" := by
  simp only [apply] at h
  unfold decision at h
  split at h
  · contradiction
  · split at h
    · split at h <;> contradiction
    · split at h
      · contradiction
      · split at h
        · contradiction
        · split at h
          · contradiction
          · simpa [kind] using h

theorem applied_memory_owned_authorized_monotonic (s : State) (r : Command)
    (kind : r.kind = .memory) (h : (apply s r).2 = "applied") :
    r.publisher ∈ s.publishers ∧ r.publisher = r.origin ∧
      r.shared = contentVersion s r + 1 :=
  memory_accepted_authorized_and_next s r (applied_memory_decision s r kind h)

theorem memory_accept_projects_shared_and_local (s : State) (r : Command)
    (kind : r.kind = .memory) :
    latest (accept s r) .memory r.origin r.canonical = some r := by
  simp [latest, accept, kind]

theorem state_accept_preserves_content (s : State) (r : Command)
    (kind : r.kind = .state) (origin canonical : Nat) :
    latest (accept s r) .memory origin canonical = latest s .memory origin canonical := by
  simp [latest, accept, kind]

theorem state_accepted_next_revision (s : State) (r : Command)
    (h : stateDecision s r = "applied") :
    r.shared = contentVersion s r ∧ r.revision = stateRevision s r + 1 := by
  unfold stateDecision at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · split at h
      · contradiction
      · split at h
        · contradiction
        · exact ⟨by omega, counter_accepted_next _ _ _ (by decide) h⟩

theorem revoke_blocks_new_memory (s : State) (rev r : Command)
    (kind : rev.kind = .revoke) (subject : r.publisher = rev.subject) :
    memoryDecision (accept s rev) r = "unauthorized" := by
  simp [memoryDecision, accept, kind, subject]

theorem revoked_memory_not_applied (s : State) (rev r : Command)
    (kind : rev.kind = .revoke) (subject : r.publisher = rev.subject)
    (memory : r.kind = .memory) :
    (apply (accept s rev) r).2 ≠ "applied" := by
  intro applied
  have ready := applied_memory_decision (accept s rev) r memory applied
  rw [revoke_blocks_new_memory s rev r kind subject] at ready
  contradiction

-- Explicit recursion and the reference fold use the same transition, but have
-- different control structure. Neither claims arbitrary delivery permutations
-- converge without retrying quarantined gaps in authoritative order.
def replay (s : State) : List Command → State
  | [] => s
  | r :: rest => replay (apply s r).1 rest

theorem replay_equals_fold (s : State) (records : List Command) :
    replay s records = records.foldl (fun acc r => (apply acc r).1) s := by
  induction records generalizing s with
  | nil => rfl
  | cons r rest ih => simpa [replay] using ih (apply s r).1

end MemoryFederation.Publication
