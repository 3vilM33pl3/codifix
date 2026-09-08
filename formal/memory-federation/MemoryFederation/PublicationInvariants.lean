import MemoryFederation.Publication

namespace MemoryFederation.Publication

theorem applied_preflight (s : State) (r : Command)
    (h : (apply s r).2 = "applied") :
    r.causation.any (fun cause => (byKey s cause).isNone) = false ∧
      transitionDecision s r = "applied" := by
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
            · exact ⟨by simpa using ‹¬ r.causation.any (fun cause => (byKey s cause).isNone) = true›, h⟩

theorem applied_causation_exists (s : State) (r : Command) (cause : Key)
    (h : (apply s r).2 = "applied") (link : r.causation = some cause) :
    (byKey s cause).isSome = true := by
  have valid := (applied_preflight s r h).1
  cases lookup : byKey s cause <;> simp_all

theorem applied_data_requires_publishing (s : State) (r : Command)
    (data : r.kind ≠ .grant ∧ r.kind ≠ .revoke)
    (h : (apply s r).2 = "applied") : r.publisher ∈ s.publishers := by
  have ready := (applied_preflight s r h).2
  by_cases present : r.publisher ∈ s.publishers
  · exact present
  · cases kind : r.kind <;>
      simp_all [transitionDecision, memoryDecision, stateDecision, evidenceDecision,
        proposalDecision, resolveDecision, relationDecision]

theorem applied_evidence_owned_and_space_bound (s : State) (r : Command)
    (kind : r.kind = .evidence) (h : (apply s r).2 = "applied") :
    r.publisher = r.origin ∧ r.evidenceSpace = 100 := by
  have ready := (applied_preflight s r h).2
  simp only [transitionDecision, kind] at ready
  unfold evidenceDecision at ready
  split at ready
  · contradiction
  · split at ready
    · contradiction
    · split at ready
      · contradiction
      · exact ⟨by omega, by omega⟩

theorem checked_evidence_references_are_present_and_scoped
    (s : State) (origin canonical : Nat) (refs seen : List Key)
    (h : checkEvidence s origin canonical refs seen = "applied") :
    ∀ ref ∈ refs, ∃ e, byKey s ref = some e ∧ e.kind = .evidence ∧
      ref.publisher = origin ∧ e.origin = origin ∧ e.canonical = canonical ∧ e.evidenceSpace = 100 := by
  induction refs generalizing seen with
  | nil => simp
  | cons ref rest ih =>
    unfold checkEvidence at h
    split at h
    · contradiction
    · cases found : byKey s ref with
      | none => simp [found] at h
      | some e =>
        simp only [found] at h
        split at h
        · contradiction
        · rename_i kind
          split at h
          · contradiction
          · rename_i scope
            have fields : ref.publisher = origin ∧ e.origin = origin ∧
                e.canonical = canonical ∧ e.evidenceSpace = 100 := by simpa using scope
            intro target member
            rcases List.mem_cons.mp member with same | later
            · subst target
              exact ⟨e, found, by simpa using kind, fields⟩
            · exact ih (ref :: seen) h target later

-- Accepted commands are immutable; a later event with the same key cannot
-- replace evidence or an earlier proposal, even with a valid new digest.
theorem changed_binding_not_applied (s : State) (r old : Command)
    (seen : known s r = some old) (changed : old ≠ r) :
    (apply s r).2 ≠ "applied" := by
  by_cases identity : r.authenticated = r.publisher <;>
    simp [apply, decision, seen, changed, identity]

theorem proposal_application_preserves_versions (s : State) (r : Command)
    (kind : r.kind = .proposal) : (apply s r).1.versions = s.versions := by
  dsimp only [apply]
  split <;> simp [accept, nextVersions, kind]

theorem resolve_accepted_origin_pending_causal (s : State) (r p : Command)
    (found : proposalRecord s r.proposal = some p)
    (h : resolveDecision s r = "applied") :
    r.publisher = p.origin ∧ (resolution s r.proposal).isSome = false ∧
      r.causation = r.proposal := by
  simp only [resolveDecision, found] at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · split at h
      · contradiction
      · split at h
        · contradiction
        · exact ⟨by omega, by simpa using ‹¬ (resolution s r.proposal).isSome = true›, by simpa using ‹¬ r.causation ≠ r.proposal›⟩

theorem approval_requires_current_base (s : State) (r p : Command)
    (found : proposalRecord s r.proposal = some p) (approval : r.local ≠ 0)
    (h : resolveDecision s r = "applied") : contentVersion s p = p.shared := by
  simp only [resolveDecision, found, approval, ite_false] at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · split at h
      · contradiction
      · split at h
        · contradiction
        · split at h
          · contradiction
          · exact by simpa using ‹¬ contentVersion s p ≠ p.shared›

theorem stale_approval_unchanged (s : State) (r p : Command)
    (kind : r.kind = .resolve) (found : proposalRecord s r.proposal = some p)
    (approval : r.local ≠ 0) (stale : contentVersion s p ≠ p.shared) :
    (apply s r).1 = s := by
  apply rejected_unchanged
  intro applied
  have ready := (applied_preflight s r applied).2
  simp only [transitionDecision, kind] at ready
  exact stale (approval_requires_current_base s r p found approval ready)

theorem approval_preserves_content_author_evidence_and_origin_key (s : State) (r p : Command)
    (found : proposalRecord s r.proposal = some p) (approval : r.local ≠ 0) :
    ∃ v, approvedVersion s r = some v ∧ key v = key r ∧
      v.origin = p.origin ∧ v.canonical = p.canonical ∧
      v.text = p.text ∧ v.author = p.author ∧ v.evidence = p.evidence ∧
      v.local = r.local ∧ v.shared = contentVersion s p + 1 := by
  simp [approvedVersion, found, approval, key]

theorem approval_appends_atomically_with_resolution (s : State) (r v : Command)
    (kind : r.kind = .resolve) (h : (apply s r).2 = "applied")
    (projection : approvedVersion s r = some v) :
    (apply s r).1.versions = v :: s.versions ∧
      (apply s r).1.accepted = r :: s.accepted := by
  have outcome : decision s r = "applied" := h
  simp [apply, outcome, accept, nextVersions, kind, projection]

theorem rejection_never_appends_content (s : State) (r : Command)
    (kind : r.kind = .resolve) (rejection : r.local = 0) :
    (apply s r).1.versions = s.versions := by
  dsimp only [apply]
  split <;> simp [accept, nextVersions, approvedVersion, kind, rejection]

theorem relation_existing_keeps_endpoints_and_next_revision (s : State) (r old : Command)
    (found : previousRelation s r = some old) (h : relationDecision s r = "applied") :
    old.origin = r.origin ∧ old.canonical = r.canonical ∧
      old.targetOrigin = r.targetOrigin ∧ old.targetCanonical = r.targetCanonical ∧
      old.relationType = r.relationType ∧ r.revision = old.revision + 1 := by
  simp only [relationDecision, found] at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · split at h
      · contradiction
      · split at h
        · contradiction
        · rename_i shape
          have fields : old.origin = r.origin ∧ old.canonical = r.canonical ∧
              old.targetOrigin = r.targetOrigin ∧ old.targetCanonical = r.targetCanonical ∧
              old.relationType = r.relationType := by
            simpa using shape
          exact ⟨fields.1, fields.2.1, fields.2.2.1, fields.2.2.2.1, fields.2.2.2.2,
            counter_accepted_next old.revision r.revision "revision_gap" (by decide) h⟩

theorem relation_updates_preserve_content (s : State) (r : Command)
    (kind : r.kind = .relation) : (apply s r).1.versions = s.versions := by
  dsimp only [apply]
  split <;> simp [accept, nextVersions, kind]

theorem applied_membership_requires_configured_authority (s : State) (r : Command)
    (kind : r.kind = .grant ∨ r.kind = .revoke) (h : (apply s r).2 = "applied") :
    r.publisher = 1 ∧ 1 ∈ s.managers := by
  have ready := (applied_preflight s r h).2
  rcases kind with grant | revoke
  all_goals
    simp only [transitionDecision, *] at ready
    split at ready
    · contradiction
    · simpa using ‹¬ (r.publisher ≠ 1 ∨ 1 ∉ s.managers)›

end MemoryFederation.Publication
