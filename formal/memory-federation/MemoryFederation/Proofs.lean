import MemoryFederation.Model

namespace MemoryFederation

theorem duplicate_replay_idempotent
    (state : ReplicaState)
    (record : TraceRecord)
    (hSeen : record.requestId ∈ state.seenRequests) :
    applyRecord state record = { state := state, outcome := .duplicateReplay } := by
  simp [applyRecord, hSeen]

theorem versionsFor_addVersion_same
    (histories : List MemoryHistory)
    (canonicalId : CanonicalId)
    (version : MemoryVersion) :
    versionsFor (addVersion histories canonicalId version) canonicalId =
      version :: versionsFor histories canonicalId := by
  induction histories with
  | nil =>
      simp [addVersion, versionsFor]
  | cons history rest ih =>
      by_cases hMatch : history.canonicalId = canonicalId
      · simp [addVersion, versionsFor, hMatch]
      · simp [addVersion, versionsFor, hMatch, ih]

theorem toMemory_some_fields
    (record : TraceRecord)
    (version : MemoryVersion)
    (hMemory : toMemory? record = some version) :
    record.canonicalId = some version.canonicalId ∧
      record.version = some version.version ∧
      record.author = some version.author ∧
      record.source = some version.source ∧
      record.locator = some version.locator ∧
      record.digest = some version.digest ∧
      record.publisher = version.publisher ∧
      record.evidenceRefs = version.evidenceRefs := by
  cases record with
  | mk requestId authority publisher kind subject capability evidenceId canonicalId versionNumber
      author source locator digest evidenceRefs tombstone =>
      cases hCanonicalId : canonicalId with
      | none =>
          simp [toMemory?, hCanonicalId] at hMemory
      | some canonicalIdValue =>
          cases hVersion : versionNumber with
          | none =>
              simp [toMemory?, hCanonicalId, hVersion] at hMemory
          | some versionValue =>
              cases hAuthor : author with
              | none =>
                  simp [toMemory?, hCanonicalId, hVersion, hAuthor] at hMemory
              | some authorValue =>
                  cases hSource : source with
                  | none =>
                      simp [toMemory?, hCanonicalId, hVersion, hAuthor, hSource] at hMemory
                  | some sourceValue =>
                      cases hLocator : locator with
                      | none =>
                          simp [toMemory?, hCanonicalId, hVersion, hAuthor, hSource, hLocator] at hMemory
                      | some locatorValue =>
                          cases hDigest : digest with
                          | none =>
                              simp [toMemory?, hCanonicalId, hVersion, hAuthor, hSource, hLocator, hDigest] at hMemory
                          | some digestValue =>
                              simp [toMemory?, hCanonicalId, hVersion, hAuthor, hSource, hLocator, hDigest] at hMemory
                              cases hMemory
                              simp [toMemory?, hCanonicalId, hVersion, hAuthor, hSource, hLocator, hDigest]

theorem toEvidence_some_publisher
    (record : TraceRecord)
    (evidence : EvidenceRecord)
    (hEvidence : toEvidence? record = some evidence) :
    record.publisher = evidence.publisher := by
  cases record with
  | mk requestId authority publisher kind subject capability recordEvidenceId canonicalId version author
      source locator digest evidenceRefs tombstone =>
      cases hEvidenceId : recordEvidenceId with
      | none =>
          simp [toEvidence?, hEvidenceId] at hEvidence
      | some evidenceIdValue =>
          cases hSource : source with
          | none =>
              simp [toEvidence?, hEvidenceId, hSource] at hEvidence
          | some sourceValue =>
              cases hLocator : locator with
              | none =>
                  simp [toEvidence?, hEvidenceId, hSource, hLocator] at hEvidence
              | some locatorValue =>
                  cases hDigest : digest with
                  | none =>
                      simp [toEvidence?, hEvidenceId, hSource, hLocator, hDigest] at hEvidence
                  | some digestValue =>
                      simp [toEvidence?, hEvidenceId, hSource, hLocator, hDigest] at hEvidence
                      cases hEvidence
                      rfl

theorem applyRecord_ready
    (state : ReplicaState)
    (record : TraceRecord)
    (hSeen : record.requestId ∉ state.seenRequests)
    (hAuthority : authorityValid record = true) :
    applyRecord state record = applyAuthorizedRecord state record := by
  simp [applyRecord, hSeen, hAuthority]

theorem applyEvidenceRecord_accepted_projection
    (state : ReplicaState)
    (record : TraceRecord)
    (projection : AppliedProjection)
    (hOutcome : (applyEvidenceRecord state record).outcome = .acceptedData projection) :
    ∃ evidence : EvidenceRecord, projection = .evidence evidence := by
  by_cases hRevoked : hasRevokedPublisher state record.publisher = true
  · simp [applyEvidenceRecord, hRevoked] at hOutcome
  · have hRevokedFalse : hasRevokedPublisher state record.publisher = false := by
      cases hRaw : hasRevokedPublisher state record.publisher <;> simp_all
    by_cases hActive : hasActivePublisher state record.publisher = true
    · cases hEvidence : toEvidence? record with
      | none =>
          simp [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence] at hOutcome
      | some parsed =>
          by_cases hConflict : conflictingDigest state parsed.source parsed.locator parsed.digest = true
          · simp [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence, hConflict] at hOutcome
          · have hConflictFalse : conflictingDigest state parsed.source parsed.locator parsed.digest = false := by
              cases hRaw : conflictingDigest state parsed.source parsed.locator parsed.digest <;> simp_all
            have hParsed : AppliedProjection.evidence parsed = projection := by
              simpa [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence, hConflictFalse] using hOutcome
            cases hParsed
            exact ⟨parsed, rfl⟩
    · have hActiveFalse : hasActivePublisher state record.publisher = false := by
        cases hRaw : hasActivePublisher state record.publisher <;> simp_all
      simp [applyEvidenceRecord, hRevokedFalse, hActiveFalse] at hOutcome

theorem applyMemoryRecord_accepted_projection
    (state : ReplicaState)
    (record : TraceRecord)
    (projection : AppliedProjection)
    (hOutcome : (applyMemoryRecord state record).outcome = .acceptedData projection) :
    ∃ version : MemoryVersion, projection = .memory version := by
  by_cases hRevoked : hasRevokedPublisher state record.publisher = true
  · simp [applyMemoryRecord, hRevoked] at hOutcome
  · have hRevokedFalse : hasRevokedPublisher state record.publisher = false := by
      cases hRaw : hasRevokedPublisher state record.publisher <;> simp_all
    by_cases hActive : hasActivePublisher state record.publisher = true
    · cases hMemory : toMemory? record with
      | none =>
          simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory] at hOutcome
      | some parsed =>
          by_cases hConflict : conflictingDigest state parsed.source parsed.locator parsed.digest = true
          · simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflict] at hOutcome
          · have hConflictFalse : conflictingDigest state parsed.source parsed.locator parsed.digest = false := by
              cases hRaw : conflictingDigest state parsed.source parsed.locator parsed.digest <;> simp_all
            by_cases hEvidence : allEvidencePresent state parsed.evidenceRefs = true
            · by_cases hVersion : parsed.version = currentVersion state parsed.canonicalId + 1
              · have hParsed : AppliedProjection.memory parsed = projection := by
                  simpa [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence, hVersion] using hOutcome
                cases hParsed
                exact ⟨parsed, rfl⟩
              · simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence, hVersion] at hOutcome
            · simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence] at hOutcome
    · have hActiveFalse : hasActivePublisher state record.publisher = false := by
        cases hRaw : hasActivePublisher state record.publisher <;> simp_all
      simp [applyMemoryRecord, hRevokedFalse, hActiveFalse] at hOutcome

theorem applyEvidenceRecord_accepted_inversion
    (state nextState : ReplicaState)
    (record : TraceRecord)
    (evidence : EvidenceRecord)
    (hAccepted : applyEvidenceRecord state record =
      { state := nextState, outcome := .acceptedData (.evidence evidence) }) :
    hasRevokedPublisher state record.publisher = false ∧
      hasActivePublisher state record.publisher = true ∧
      toEvidence? record = some evidence ∧
      conflictingDigest state evidence.source evidence.locator evidence.digest = false ∧
      nextState =
        markSeen
          { state with
            evidence := evidence :: state.evidence
            seenLocatorDigests :=
              insertUnique (AppliedProjection.evidence evidence).locatorKey state.seenLocatorDigests }
          record.requestId := by
  have hOutcome : (applyEvidenceRecord state record).outcome = .acceptedData (.evidence evidence) := by
    simpa using congrArg StepResult.outcome hAccepted
  have hState : (applyEvidenceRecord state record).state = nextState := by
    simpa using congrArg StepResult.state hAccepted
  by_cases hRevoked : hasRevokedPublisher state record.publisher = true
  · simp [applyEvidenceRecord, hRevoked] at hOutcome
  · have hRevokedFalse : hasRevokedPublisher state record.publisher = false := by
      cases hRaw : hasRevokedPublisher state record.publisher <;> simp_all
    by_cases hActive : hasActivePublisher state record.publisher = true
    · cases hEvidence : toEvidence? record with
      | none =>
          simp [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence] at hOutcome
      | some parsed =>
          by_cases hConflict : conflictingDigest state parsed.source parsed.locator parsed.digest = true
          · simp [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence, hConflict] at hOutcome
          · have hConflictFalse : conflictingDigest state parsed.source parsed.locator parsed.digest = false := by
              cases hRaw : conflictingDigest state parsed.source parsed.locator parsed.digest <;> simp_all
            have hParsed : parsed = evidence := by
              simpa [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence, hConflictFalse] using hOutcome
            cases hParsed
            have hStateAccepted :
                markSeen
                  { state with
                    evidence := evidence :: state.evidence
                    seenLocatorDigests :=
                      insertUnique (AppliedProjection.evidence evidence).locatorKey state.seenLocatorDigests }
                  record.requestId = nextState := by
              simpa [applyEvidenceRecord, hRevokedFalse, hActive, hEvidence, hConflictFalse] using hState
            exact ⟨hRevokedFalse, hActive, by simpa using hEvidence, hConflictFalse, hStateAccepted.symm⟩
    · have hActiveFalse : hasActivePublisher state record.publisher = false := by
        cases hRaw : hasActivePublisher state record.publisher <;> simp_all
      simp [applyEvidenceRecord, hRevokedFalse, hActiveFalse] at hOutcome

theorem applyMemoryRecord_accepted_inversion
    (state nextState : ReplicaState)
    (record : TraceRecord)
    (version : MemoryVersion)
    (hAccepted : applyMemoryRecord state record =
      { state := nextState, outcome := .acceptedData (.memory version) }) :
    hasRevokedPublisher state record.publisher = false ∧
      hasActivePublisher state record.publisher = true ∧
      toMemory? record = some version ∧
      conflictingDigest state version.source version.locator version.digest = false ∧
      allEvidencePresent state version.evidenceRefs = true ∧
      version.version = currentVersion state version.canonicalId + 1 ∧
      nextState =
        markSeen
          { state with
            histories := addVersion state.histories version.canonicalId version
            seenLocatorDigests :=
              insertUnique (AppliedProjection.memory version).locatorKey state.seenLocatorDigests }
          record.requestId := by
  have hOutcome : (applyMemoryRecord state record).outcome = .acceptedData (.memory version) := by
    simpa using congrArg StepResult.outcome hAccepted
  have hState : (applyMemoryRecord state record).state = nextState := by
    simpa using congrArg StepResult.state hAccepted
  by_cases hRevoked : hasRevokedPublisher state record.publisher = true
  · simp [applyMemoryRecord, hRevoked] at hOutcome
  · have hRevokedFalse : hasRevokedPublisher state record.publisher = false := by
      cases hRaw : hasRevokedPublisher state record.publisher <;> simp_all
    by_cases hActive : hasActivePublisher state record.publisher = true
    · cases hMemory : toMemory? record with
      | none =>
          simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory] at hOutcome
      | some parsed =>
          by_cases hConflict : conflictingDigest state parsed.source parsed.locator parsed.digest = true
          · simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflict] at hOutcome
          · have hConflictFalse : conflictingDigest state parsed.source parsed.locator parsed.digest = false := by
              cases hRaw : conflictingDigest state parsed.source parsed.locator parsed.digest <;> simp_all
            by_cases hEvidence : allEvidencePresent state parsed.evidenceRefs = true
            · by_cases hVersion : parsed.version = currentVersion state parsed.canonicalId + 1
              · have hParsed : parsed = version := by
                  simpa [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence, hVersion] using hOutcome
                cases hParsed
                have hStateAccepted :
                    markSeen
                      { state with
                        histories := addVersion state.histories version.canonicalId version
                        seenLocatorDigests :=
                          insertUnique (AppliedProjection.memory version).locatorKey state.seenLocatorDigests }
                      record.requestId = nextState := by
                  simpa [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence, hVersion] using hState
                exact ⟨hRevokedFalse, hActive, by simpa using hMemory, hConflictFalse, hEvidence, hVersion, hStateAccepted.symm⟩
              · simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence, hVersion] at hOutcome
            · simp [applyMemoryRecord, hRevokedFalse, hActive, hMemory, hConflictFalse, hEvidence] at hOutcome
    · have hActiveFalse : hasActivePublisher state record.publisher = false := by
        cases hRaw : hasActivePublisher state record.publisher <;> simp_all
      simp [applyMemoryRecord, hRevokedFalse, hActiveFalse] at hOutcome

theorem applied_data_implies_active_publisher
    (state nextState : ReplicaState)
    (record : TraceRecord)
    (projection : AppliedProjection)
    (hApplied : applyRecord state record = { state := nextState, outcome := .acceptedData projection }) :
    hasActivePublisher state projection.publisher = true := by
  have hOutcome : (applyRecord state record).outcome = .acceptedData projection := by
    simpa using congrArg StepResult.outcome hApplied
  by_cases hSeen : record.requestId ∈ state.seenRequests
  · simp [applyRecord, hSeen] at hOutcome
  · by_cases hAuthority : authorityValid record = true
    · cases hKind : record.kind with
      | grant =>
          have : False := by
            cases hSubject : record.subject <;> cases hCapability : record.capability <;>
              simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind, applyGrantRecord, hSubject, hCapability] using hOutcome
          exact False.elim this
      | revoke =>
          have : False := by
            cases hSubject : record.subject <;> cases hCapability : record.capability <;>
              simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind, applyRevokeRecord, hSubject, hCapability] using hOutcome
          exact False.elim this
      | evidence =>
          have hOutcomeAuthorized : (applyEvidenceRecord state record).outcome = .acceptedData projection := by
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using hOutcome
          rcases applyEvidenceRecord_accepted_projection state record projection hOutcomeAuthorized with
            ⟨evidence, hProjection⟩
          have hAcceptedEvidence : applyEvidenceRecord state record =
              { state := nextState, outcome := .acceptedData (.evidence evidence) } := by
            rw [hProjection] at hApplied
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using hApplied
          rcases applyEvidenceRecord_accepted_inversion state nextState record evidence hAcceptedEvidence with
            ⟨_, hActive, hEvidence, _, _⟩
          have hPublisher := toEvidence_some_publisher record evidence hEvidence
          rw [hProjection]
          simpa [AppliedProjection.publisher, hPublisher] using hActive
      | memory =>
          have hOutcomeAuthorized : (applyMemoryRecord state record).outcome = .acceptedData projection := by
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using hOutcome
          rcases applyMemoryRecord_accepted_projection state record projection hOutcomeAuthorized with
            ⟨version, hProjection⟩
          have hAcceptedMemory : applyMemoryRecord state record =
              { state := nextState, outcome := .acceptedData (.memory version) } := by
            rw [hProjection] at hApplied
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using hApplied
          rcases applyMemoryRecord_accepted_inversion state nextState record version hAcceptedMemory with
            ⟨_, hActive, hMemory, _, _, _, _⟩
          rcases toMemory_some_fields record version hMemory with
            ⟨_, _, _, _, _, _, hPublisher, _⟩
          rw [hProjection]
          simpa [AppliedProjection.publisher, hPublisher] using hActive
    · simp [applyRecord, hSeen, hAuthority] at hOutcome

theorem accepted_versions_monotonic
    (state nextState : ReplicaState)
    (record : TraceRecord)
    (version : MemoryVersion)
    (hAccepted : applyRecord state record =
      { state := nextState, outcome := .acceptedData (.memory version) }) :
    currentVersion state version.canonicalId < version.version ∧
      versionsFor nextState.histories version.canonicalId =
        version :: versionsFor state.histories version.canonicalId := by
  by_cases hSeen : record.requestId ∈ state.seenRequests
  · simp [applyRecord, hSeen] at hAccepted
  · by_cases hAuthority : authorityValid record = true
    · cases hKind : record.kind with
      | grant =>
          have : False := by
            cases hSubject : record.subject <;> cases hCapability : record.capability <;>
              simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind, applyGrantRecord, hSubject, hCapability] using congrArg StepResult.outcome hAccepted
          exact False.elim this
      | revoke =>
          have : False := by
            cases hSubject : record.subject <;> cases hCapability : record.capability <;>
              simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind, applyRevokeRecord, hSubject, hCapability] using congrArg StepResult.outcome hAccepted
          exact False.elim this
      | evidence =>
          have hOutcomeAuthorized : (applyEvidenceRecord state record).outcome = .acceptedData (.memory version) := by
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using congrArg StepResult.outcome hAccepted
          have : False := by
            rcases applyEvidenceRecord_accepted_projection state record (.memory version) hOutcomeAuthorized with
              ⟨evidence, hProjection⟩
            cases hProjection
          exact False.elim this
      | memory =>
          have hAcceptedMemory : applyMemoryRecord state record =
              { state := nextState, outcome := .acceptedData (.memory version) } := by
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using hAccepted
          rcases applyMemoryRecord_accepted_inversion state nextState record version hAcceptedMemory with
            ⟨_, _, _, _, _, hVersion, hState⟩
          constructor
          · have hLt : currentVersion state version.canonicalId <
                currentVersion state version.canonicalId + 1 :=
                Nat.lt_succ_self (currentVersion state version.canonicalId)
            simpa [hVersion] using hLt
          · rw [hState]
            exact versionsFor_addVersion_same state.histories version.canonicalId version
    · simp [applyRecord, hSeen, hAuthority] at hAccepted

theorem accepted_projection_preserves_author_locator_digest_evidence
    (state nextState : ReplicaState)
    (record : TraceRecord)
    (version : MemoryVersion)
    (hAccepted : applyRecord state record =
      { state := nextState, outcome := .acceptedData (.memory version) }) :
    record.author = some version.author ∧
      record.source = some version.source ∧
      record.locator = some version.locator ∧
        record.digest = some version.digest ∧ record.evidenceRefs = version.evidenceRefs := by
  by_cases hSeen : record.requestId ∈ state.seenRequests
  · simp [applyRecord, hSeen] at hAccepted
  · by_cases hAuthority : authorityValid record = true
    · cases hKind : record.kind with
      | grant =>
          have : False := by
            cases hSubject : record.subject <;> cases hCapability : record.capability <;>
              simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind, applyGrantRecord, hSubject, hCapability] using congrArg StepResult.outcome hAccepted
          exact False.elim this
      | revoke =>
          have : False := by
            cases hSubject : record.subject <;> cases hCapability : record.capability <;>
              simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind, applyRevokeRecord, hSubject, hCapability] using congrArg StepResult.outcome hAccepted
          exact False.elim this
      | evidence =>
          have hOutcomeAuthorized : (applyEvidenceRecord state record).outcome = .acceptedData (.memory version) := by
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using congrArg StepResult.outcome hAccepted
          have : False := by
            rcases applyEvidenceRecord_accepted_projection state record (.memory version) hOutcomeAuthorized with
              ⟨evidence, hProjection⟩
            cases hProjection
          exact False.elim this
      | memory =>
          have hAcceptedMemory : applyMemoryRecord state record =
              { state := nextState, outcome := .acceptedData (.memory version) } := by
            simpa [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hKind] using hAccepted
          rcases applyMemoryRecord_accepted_inversion state nextState record version hAcceptedMemory with
            ⟨_, _, hMemory, _, _, _, _⟩
          rcases toMemory_some_fields record version hMemory with
            ⟨_, _, hAuthor, hSource, hLocator, hDigest, _, hEvidenceRefs⟩
          exact ⟨hAuthor, hSource, hLocator, hDigest, hEvidenceRefs⟩
    · simp [applyRecord, hSeen, hAuthority] at hAccepted

theorem revocation_blocks_later_writes_while_prior_state_remains
    (state : ReplicaState)
    (record : TraceRecord)
    (publisher : PrincipalId)
    (hSeen : record.requestId ∉ state.seenRequests)
    (hAuthority : authorityValid record = true)
    (hPublisher : record.publisher = publisher)
    (hKind : record.kind = .evidence ∨ record.kind = .memory)
    (hRevoked : hasRevokedPublisher state publisher = true) :
    applyRecord state record = { state := state, outcome := .revokedPublisher } := by
  cases hKind with
  | inl hEvidence =>
      simp [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hEvidence, applyEvidenceRecord, hPublisher, hRevoked]
  | inr hMemory =>
      simp [applyRecord, hSeen, hAuthority, applyAuthorizedRecord, hMemory, applyMemoryRecord, hPublisher, hRevoked]

theorem replayTrace_eq_referenceFoldFrom
    (run : TraceRun)
    (records : List TraceRecord) :
    referenceFoldFrom run records =
      let replay := replayTrace run.finalState records
      { finalState := replay.finalState, outcomes := run.outcomes ++ replay.outcomes } := by
  induction records generalizing run with
  | nil =>
      simp [referenceFoldFrom, replayTrace]
  | cons record rest ih =>
      have hTail :=
        ih {
          finalState := (applyRecord run.finalState record).state
          outcomes := run.outcomes ++ [(applyRecord run.finalState record).outcome.tag]
        }
      simpa [referenceFoldFrom, replayTrace, extendRun, List.foldl, List.append_assoc] using hTail

theorem replay_equals_reference_fold
    (state : ReplicaState)
    (records : List TraceRecord) :
    replayTrace state records = referenceFold state records := by
  simpa [referenceFold] using
    (replayTrace_eq_referenceFoldFrom { finalState := state, outcomes := [] } records).symm

end MemoryFederation
