import MemoryFederation.PublicationInvariants

/- The advertisement part of F1, under the publication model's prerequisites.
   These theorems do not model HTTP, review UI, cryptography, model execution,
   source freshness, mailbox leases or gateway policy freshness. -/
namespace MemoryFederation.Publication

theorem expertise_owned_authorized_monotonic (s : State) (r : Command)
    (h : expertiseDecision s r = "applied") :
    r.publisher ∈ s.publishers ∧ r.publisher = r.origin ∧
    (r.active = true → r.publisher ∈ s.answerers) ∧
    r.revision = (((latest s .expertise r.origin r.canonical).map (·.revision)).getD 0) + 1 := by
  unfold expertiseDecision at h
  split at h
  · contradiction
  · rename_i publish
    split at h
    · contradiction
    · rename_i owner
      split at h
      · contradiction
      · rename_i answering
        refine ⟨by simpa using publish, by simpa using owner, ?_, ?_⟩
        · intro active
          simpa [active] using answering
        · exact counter_accepted_next _ _ _ (by decide) h

theorem applied_expertise_owned_authorized_monotonic (s : State) (r : Command)
    (kind : r.kind = .expertise) (h : (apply s r).2 = "applied") :
    r.publisher ∈ s.publishers ∧ r.publisher = r.origin ∧
    (r.active = true → r.publisher ∈ s.answerers) ∧
    r.revision = (((latest s .expertise r.origin r.canonical).map (·.revision)).getD 0) + 1 := by
  have ready := (applied_preflight s r h).2
  exact expertise_owned_authorized_monotonic s r (by simpa [transitionDecision, kind] using ready)

theorem revoked_answerer_cannot_activate (s : State) (rev r : Command)
    (kind : rev.kind = .revoke) (subject : r.publisher = rev.subject)
    (capability : rev.capability = "answer_expertise" ∨ rev.capability = "")
    (active : r.active = true) : expertiseDecision (accept s rev) r ≠ "applied" := by
  intro h
  have authorized := (expertise_owned_authorized_monotonic (accept s rev) r h).2.2.1 active
  simp [accept, changeMembers, kind, subject, capability] at authorized

theorem expertise_does_not_change_memory (s : State) (r : Command)
    (kind : r.kind = .expertise) : (apply s r).1.versions = s.versions := by
  dsimp only [apply]
  split <;> simp [accept, nextVersions, kind]

-- Pure decision fragment shared with the Rust query authorization tests:
-- caller authenticity, fresh policy/time and source eligibility are additional
-- runtime obligations, not implications of space membership.
def mayAsk (s : State) (requester responder : Nat) (active recipientAllowed : Bool) : Bool :=
  decide (requester ∈ s.askers ∧ requester ∈ s.pullers ∧
    responder ∈ s.answerers ∧ responder ∈ s.pullers) && active && recipientAllowed

theorem revoked_asker_denied (s : State) (rev : Command) (responder : Nat)
    (kind : rev.kind = .revoke)
    (capability : rev.capability = "ask_expertise" ∨ rev.capability = "") :
    mayAsk (accept s rev) rev.subject responder true true = false := by
  simp [mayAsk, accept, changeMembers, kind, capability]

theorem collection_withdrawn_denies (s : State) (requester responder : Nat) :
    mayAsk s requester responder false true = false := by simp [mayAsk]

end MemoryFederation.Publication
