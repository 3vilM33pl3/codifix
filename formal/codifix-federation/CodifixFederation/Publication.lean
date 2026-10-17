import Lean

open Lean

/- A second, explicitly scoped model for the production-shaped publication
   domain. Natural-number IDs abstract validated UUIDs; counters abstract
   positive u32 inputs. Digests/authentication are established before this model.
   Evidence bytes/review digests are validated before this model. Durable storage
   and transport cryptography are not modeled here. -/
namespace CodifixFederation.Publication

inductive Kind where
  | grant | revoke | memory | state | evidence | proposal | resolve | relation | expertise
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, FromJson

structure Key where
  publisher : Nat
  id : Nat
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
  capability : String := "publish"
  causation : Option Key := none
  evidence : List Key := []
  evidenceSpace : Nat := 100
  proposal : Option Key := none
  relationId : Nat := 0
  targetOrigin : Nat := 0
  targetCanonical : Nat := 0
  relationType : String := "related_to"
  active : Bool := true
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
      capability := ← fieldDefault j "capability" "publish"
      causation := ← fieldDefault j "causation" none
      evidence := ← fieldDefault j "evidence" []
      evidenceSpace := ← fieldDefault j "evidenceSpace" 100
      proposal := ← fieldDefault j "proposal" none
      relationId := ← fieldDefault j "relationId" 0
      targetOrigin := ← fieldDefault j "targetOrigin" 0
      targetCanonical := ← fieldDefault j "targetCanonical" 0
      relationType := ← fieldDefault j "relationType" "related_to"
      active := ← fieldDefault j "active" true
    }

structure State where
  sequence : Nat := 0
  publishers : List Nat := [1]
  pullers : List Nat := [1]
  managers : List Nat := [1]
  askers : List Nat := []
  answerers : List Nat := []
  versions : List Command := []
  -- Newest first; every accepted command is kept verbatim.
  accepted : List Command := []
  deriving Repr, DecidableEq, BEq, Inhabited

def latest (s : State) (kind : Kind) (origin canonical : Nat) : Option Command :=
  (if kind = .memory then s.versions else s.accepted).find?
    fun r => r.kind == kind && r.origin == origin && r.canonical == canonical

def key (r : Command) : Key := ⟨r.publisher, r.id⟩

def byKey (s : State) (k : Key) : Option Command :=
  s.accepted.find? fun r => key r == k

def evidenceDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if r.publisher ≠ r.origin then "not_origin"
  else if r.evidenceSpace ≠ 100 then "evidence_scope"
  else "applied"

def checkEvidence (s : State) (origin canonical : Nat) : List Key → List Key → String
  | [], _ => "applied"
  | ref :: rest, seen =>
    if ref ∈ seen then "duplicate_evidence"
    else match byKey s ref with
    | none => "missing_dependency"
    | some e =>
      if e.kind ≠ .evidence then "missing_dependency"
      else if ref.publisher ≠ origin ∨ e.origin ≠ origin ∨ e.canonical ≠ canonical ∨ e.evidenceSpace ≠ 100
        then "evidence_scope"
      else checkEvidence s origin canonical rest (ref :: seen)

def proposalRecord (s : State) (k : Option Key) : Option Command :=
  k.bind fun key => (byKey s key).filter (·.kind == .proposal)

def resolution (s : State) (k : Option Key) : Option Command :=
  s.accepted.find? fun r => r.kind == .resolve && r.proposal == k

def previousRelation (s : State) (r : Command) : Option Command :=
  s.accepted.find? fun old => old.kind == .relation && old.publisher == r.publisher && old.relationId == r.relationId

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
  else let counter := counterDecision (contentVersion s r) r.shared "version_gap"
       if counter ≠ "applied" then counter else checkEvidence s r.origin r.canonical r.evidence []

def stateDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if r.publisher ≠ r.origin then "not_origin"
  else if contentVersion s r = 0 ∨ r.shared > contentVersion s r then "missing_dependency"
  else if r.shared < contentVersion s r then "counter_conflict"
  else counterDecision (stateRevision s r) r.revision "revision_gap"

def proposalDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if contentVersion s r = 0 ∨ r.shared > contentVersion s r then "missing_dependency"
  else if r.shared < contentVersion s r then "stale_proposal"
  else checkEvidence s r.origin r.canonical r.evidence []

def resolveDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else match proposalRecord s r.proposal with
  | none => "missing_dependency"
  | some p =>
    if r.publisher ≠ p.origin then "not_origin"
    else if (resolution s r.proposal).isSome then "proposal_closed"
    else if r.causation ≠ r.proposal then "invalid_causation"
    else if r.local = 0 then "applied" -- reject proposal, including stale ones
    else if contentVersion s p ≠ p.shared then "stale_proposal"
    else if contentVersion s p ≥ 4294967295 then "counter_exhausted"
    else checkEvidence s p.origin p.canonical p.evidence []

def relationDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if r.publisher ≠ r.origin then "not_origin"
  else if contentVersion s r = 0 ∨ (latest s .memory r.targetOrigin r.targetCanonical).isNone then "missing_dependency"
  else match previousRelation s r with
  | none => if !r.active then "missing_dependency" else counterDecision 0 r.revision "revision_gap"
  | some old =>
    if old.origin ≠ r.origin ∨ old.canonical ≠ r.canonical ∨ old.targetOrigin ≠ r.targetOrigin ∨
       old.targetCanonical ≠ r.targetCanonical ∨ old.relationType ≠ r.relationType then "relation_conflict"
    else counterDecision old.revision r.revision "revision_gap"

def expertiseDecision (s : State) (r : Command) : String :=
  if r.publisher ∉ s.publishers then "unauthorized"
  else if r.publisher ≠ r.origin then "not_origin"
  else if r.active ∧ r.publisher ∉ s.answerers then "unauthorized"
  else counterDecision (((latest s .expertise r.origin r.canonical).map (·.revision)).getD 0)
    r.revision "revision_gap"

def transitionDecision (s : State) (r : Command) : String :=
  match r.kind with
  | .grant | .revoke =>
    if r.publisher ≠ 1 ∨ 1 ∉ s.managers then "unauthorized"
    else if r.kind = .revoke ∧ r.subject = 1 ∧ (r.capability = "" ∨ r.capability = "manage_membership")
      then "authority_revocation"
    else "applied"
  | .memory => memoryDecision s r
  | .state => stateDecision s r
  | .evidence => evidenceDecision s r
  | .proposal => proposalDecision s r
  | .resolve => resolveDecision s r
  | .relation => relationDecision s r
  | .expertise => expertiseDecision s r

def decision (s : State) (r : Command) : String :=
  if r.authenticated ≠ r.publisher then "forged_publisher"
  else match known s r with
  | some old => if old = r then "duplicate" else "equivocation"
  | none =>
    if s.sequence ≥ 4294967295 then "counter_exhausted"
    else if r.sequence > s.sequence + 1 then "order_gap"
    else if r.sequence < s.sequence + 1 then "stale_order"
    else if r.causation.any (fun cause => (byKey s cause).isNone) then "missing_dependency"
    else transitionDecision s r

def changeMembers (members : List Nat) (capability : String) (r : Command) : List Nat :=
  if r.kind = .grant ∧ r.capability = capability then r.subject :: members.filter (· != r.subject)
  else if r.kind = .revoke ∧ (r.capability = capability ∨ r.capability = "") then members.filter (· != r.subject)
  else members

-- Approval publishes the exact proposed content/evidence and attribution, with
-- the origin's approval key and source-version provenance; the proposal itself
-- remains in the accepted log and is resolved by this same accepted command.
def approvedVersion (s : State) (r : Command) : Option Command :=
  if r.local = 0 then none else (proposalRecord s r.proposal).map fun p =>
    { r with
      kind := .memory
      origin := p.origin
      canonical := p.canonical
      shared := contentVersion s p + 1
      text := p.text
      author := p.author
      evidence := p.evidence }

def nextVersions (s : State) (r : Command) : List Command :=
  if r.kind = .memory then r :: s.versions
  else if r.kind = .resolve then
    match approvedVersion s r with
    | some version => version :: s.versions
    | none => s.versions
  else s.versions

def accept (s : State) (r : Command) : State :=
  { sequence := r.sequence, accepted := r :: s.accepted,
    publishers := changeMembers s.publishers "publish" r,
    pullers := changeMembers s.pullers "pull" r,
    managers := changeMembers s.managers "manage_membership" r,
    askers := changeMembers s.askers "ask_expertise" r,
    answerers := changeMembers s.answerers "answer_expertise" r,
    versions := nextVersions s r }

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
    · dsimp only at h
      split at h
      · contradiction
      · exact ⟨by simpa using ‹¬¬ r.publisher ∈ s.publishers›, by omega,
          counter_accepted_next (contentVersion s r) r.shared "version_gap" (by decide)
            (by simpa using ‹¬ counterDecision (contentVersion s r) r.shared "version_gap" ≠ "applied"›)⟩

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
          · split at h
            · contradiction
            · simpa [transitionDecision, kind] using h

theorem applied_memory_owned_authorized_monotonic (s : State) (r : Command)
    (kind : r.kind = .memory) (h : (apply s r).2 = "applied") :
    r.publisher ∈ s.publishers ∧ r.publisher = r.origin ∧
      r.shared = contentVersion s r + 1 :=
  memory_accepted_authorized_and_next s r (applied_memory_decision s r kind h)

theorem memory_accept_projects_shared_and_local (s : State) (r : Command)
    (kind : r.kind = .memory) :
    latest (accept s r) .memory r.origin r.canonical = some r := by
  simp [latest, accept, nextVersions, kind]

theorem state_accept_preserves_content (s : State) (r : Command)
    (kind : r.kind = .state) (origin canonical : Nat) :
    latest (accept s r) .memory origin canonical = latest s .memory origin canonical := by
  simp [latest, accept, nextVersions, kind]

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
    (kind : rev.kind = .revoke) (subject : r.publisher = rev.subject)
    (capability : rev.capability = "publish" ∨ rev.capability = "") :
    memoryDecision (accept s rev) r = "unauthorized" := by
  simp [memoryDecision, accept, changeMembers, kind, subject, capability]

theorem revoked_memory_not_applied (s : State) (rev r : Command)
    (kind : rev.kind = .revoke) (subject : r.publisher = rev.subject)
    (capability : rev.capability = "publish" ∨ rev.capability = "")
    (memory : r.kind = .memory) :
    (apply (accept s rev) r).2 ≠ "applied" := by
  intro applied
  have ready := applied_memory_decision (accept s rev) r memory applied
  rw [revoke_blocks_new_memory s rev r kind subject capability] at ready
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

end CodifixFederation.Publication
