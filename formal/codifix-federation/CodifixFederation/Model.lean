import Lean

open Lean

namespace CodifixFederation

abbrev PrincipalId := String
abbrev RequestId := String
abbrev CanonicalId := String
abbrev EvidenceId := String
abbrev SourceId := String
abbrev Locator := String
abbrev Digest := String

inductive Capability where
  | publish
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

inductive RecordKind where
  | grant
  | revoke
  | evidence
  | memory
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

inductive OutcomeTag where
  | acceptedGrant
  | acceptedRevoke
  | acceptedEvidence
  | acceptedMemory
  | duplicateReplay
  | unauthorized
  | revokedPublisher
  | versionGap
  | missingEvidence
  | equivocation
  | invalidShape
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure MembershipKey where
  principal : PrincipalId
  capability : Capability
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure LocatorDigestKey where
  source : SourceId
  locator : Locator
  digest : Digest
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure EvidenceRecord where
  evidenceId : EvidenceId
  publisher : PrincipalId
  source : SourceId
  locator : Locator
  digest : Digest
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure MemoryVersion where
  canonicalId : CanonicalId
  version : Nat
  publisher : PrincipalId
  author : PrincipalId
  source : SourceId
  locator : Locator
  digest : Digest
  evidenceRefs : List EvidenceId
  tombstone : Bool
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure MemoryHistory where
  canonicalId : CanonicalId
  versions : List MemoryVersion
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

inductive AppliedProjection where
  | evidence (value : EvidenceRecord)
  | memory (value : MemoryVersion)
  deriving Repr, DecidableEq, Inhabited

def AppliedProjection.publisher : AppliedProjection → PrincipalId
  | .evidence value => value.publisher
  | .memory value => value.publisher

def AppliedProjection.tag : AppliedProjection → OutcomeTag
  | .evidence _ => .acceptedEvidence
  | .memory _ => .acceptedMemory

def AppliedProjection.locatorKey : AppliedProjection → LocatorDigestKey
  | .evidence value =>
      { source := value.source, locator := value.locator, digest := value.digest }
  | .memory value =>
      { source := value.source, locator := value.locator, digest := value.digest }

inductive ApplyOutcome where
  | acceptedGrant (subject : PrincipalId) (capability : Capability)
  | acceptedRevoke (subject : PrincipalId) (capability : Capability)
  | acceptedData (projection : AppliedProjection)
  | duplicateReplay
  | unauthorized
  | revokedPublisher
  | versionGap
  | missingEvidence
  | equivocation
  | invalidShape
  deriving Repr, DecidableEq, Inhabited

def ApplyOutcome.tag : ApplyOutcome → OutcomeTag
  | .acceptedGrant .. => .acceptedGrant
  | .acceptedRevoke .. => .acceptedRevoke
  | .acceptedData projection => projection.tag
  | .duplicateReplay => .duplicateReplay
  | .unauthorized => .unauthorized
  | .revokedPublisher => .revokedPublisher
  | .versionGap => .versionGap
  | .missingEvidence => .missingEvidence
  | .equivocation => .equivocation
  | .invalidShape => .invalidShape

def ApplyOutcome.isAccepted : ApplyOutcome → Prop
  | .acceptedGrant .. => True
  | .acceptedRevoke .. => True
  | .acceptedData .. => True
  | _ => False

structure TraceRecord where
  requestId : RequestId
  authority : PrincipalId
  publisher : PrincipalId
  kind : RecordKind
  subject : Option PrincipalId := none
  capability : Option Capability := none
  evidenceId : Option EvidenceId := none
  canonicalId : Option CanonicalId := none
  version : Option Nat := none
  author : Option PrincipalId := none
  source : Option SourceId := none
  locator : Option Locator := none
  digest : Option Digest := none
  evidenceRefs : List EvidenceId := []
  tombstone : Bool := false
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure ReplicaState where
  seenRequests : List RequestId := []
  activeCapabilities : List MembershipKey := []
  revokedCapabilities : List MembershipKey := []
  seenLocatorDigests : List LocatorDigestKey := []
  evidence : List EvidenceRecord := []
  histories : List MemoryHistory := []
  deriving Repr, DecidableEq, Inhabited

structure StepResult where
  state : ReplicaState
  outcome : ApplyOutcome
  deriving Repr, DecidableEq, Inhabited

structure CanonicalHead where
  canonicalId : CanonicalId
  version : Nat
  publisher : PrincipalId
  author : PrincipalId
  source : SourceId
  locator : Locator
  digest : Digest
  evidenceCount : Nat
  tombstone : Bool
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure FinalSummary where
  seenRequestCount : Nat
  evidenceCount : Nat
  memoryVersionCount : Nat
  activeCanonicalCount : Nat
  activePublishers : List PrincipalId
  revokedPublishers : List PrincipalId
  canonicalHeads : List CanonicalHead
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure TraceExpectation where
  outcomes : List OutcomeTag
  summary : FinalSummary
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure TraceFixture where
  name : String
  records : List TraceRecord
  expected : TraceExpectation
  deriving Repr, DecidableEq, Inhabited, BEq, FromJson, ToJson

structure TraceRun where
  finalState : ReplicaState
  outcomes : List OutcomeTag
  deriving Repr, DecidableEq, Inhabited

def rootAuthority : PrincipalId := "root"

def insertUnique [DecidableEq α] (value : α) (xs : List α) : List α :=
  if value ∈ xs then xs else value :: xs

def publisherKey (publisher : PrincipalId) : MembershipKey :=
  { principal := publisher, capability := .publish }

def hasActivePublisher (state : ReplicaState) (publisher : PrincipalId) : Bool :=
  state.activeCapabilities.contains (publisherKey publisher)

def hasRevokedPublisher (state : ReplicaState) (publisher : PrincipalId) : Bool :=
  state.revokedCapabilities.contains (publisherKey publisher)

def authorityValid (record : TraceRecord) : Bool :=
  match record.kind with
  | .grant | .revoke =>
      record.authority == rootAuthority && record.publisher == rootAuthority
  | .evidence | .memory =>
      record.authority == record.publisher

def markSeen (state : ReplicaState) (requestId : RequestId) : ReplicaState :=
  { state with seenRequests := insertUnique requestId state.seenRequests }

def grantCapability (state : ReplicaState) (subject : PrincipalId) (capability : Capability) :
    ReplicaState :=
  let key := { principal := subject, capability := capability }
  { state with
      activeCapabilities := insertUnique key state.activeCapabilities
      revokedCapabilities := state.revokedCapabilities.erase key }

def revokeCapability (state : ReplicaState) (subject : PrincipalId) (capability : Capability) :
    ReplicaState :=
  let key := { principal := subject, capability := capability }
  { state with
      activeCapabilities := state.activeCapabilities.erase key
      revokedCapabilities := insertUnique key state.revokedCapabilities }

def toEvidence? (record : TraceRecord) : Option EvidenceRecord :=
  do
    let evidenceId ← record.evidenceId
    let source ← record.source
    let locator ← record.locator
    let digest ← record.digest
    pure {
      evidenceId := evidenceId
      publisher := record.publisher
      source := source
      locator := locator
      digest := digest
    }

def toMemory? (record : TraceRecord) : Option MemoryVersion :=
  do
    let canonicalId ← record.canonicalId
    let version ← record.version
    let author ← record.author
    let source ← record.source
    let locator ← record.locator
    let digest ← record.digest
    pure {
      canonicalId := canonicalId
      version := version
      publisher := record.publisher
      author := author
      source := source
      locator := locator
      digest := digest
      evidenceRefs := record.evidenceRefs
      tombstone := record.tombstone
    }

def versionsFor : List MemoryHistory → CanonicalId → List MemoryVersion
  | [], _ => []
  | history :: rest, canonicalId =>
      if history.canonicalId = canonicalId then history.versions else versionsFor rest canonicalId

def currentVersion (state : ReplicaState) (canonicalId : CanonicalId) : Nat :=
  match versionsFor state.histories canonicalId with
  | version :: _ => version.version
  | [] => 0

def addVersion : List MemoryHistory → CanonicalId → MemoryVersion → List MemoryHistory
  | [], canonicalId, version =>
      [{ canonicalId := canonicalId, versions := [version] }]
  | history :: rest, canonicalId, version =>
      if history.canonicalId = canonicalId then
        { history with versions := version :: history.versions } :: rest
      else
        history :: addVersion rest canonicalId version

def memoryVersionCount : List MemoryHistory → Nat
  | [] => 0
  | history :: rest => history.versions.length + memoryVersionCount rest

def latestHeads : List MemoryHistory → List CanonicalHead
  | [] => []
  | history :: rest =>
      match history.versions with
      | version :: _ =>
          {
            canonicalId := version.canonicalId
            version := version.version
            publisher := version.publisher
            author := version.author
            source := version.source
            locator := version.locator
            digest := version.digest
            evidenceCount := version.evidenceRefs.length
            tombstone := version.tombstone
          } :: latestHeads rest
      | [] => latestHeads rest

def activeCanonicalCount (histories : List MemoryHistory) : Nat :=
  (latestHeads histories).foldl
    (fun count head => if head.tombstone then count else count + 1)
    0

def conflictingDigest (state : ReplicaState) (source : SourceId) (locator : Locator)
    (digest : Digest) : Bool :=
  state.seenLocatorDigests.any fun key =>
    key.source == source && key.locator == locator && key.digest != digest

def evidencePresent (state : ReplicaState) (evidenceId : EvidenceId) : Bool :=
  state.evidence.any fun entry => entry.evidenceId == evidenceId

def allEvidencePresent (state : ReplicaState) (references : List EvidenceId) : Bool :=
  references.all (evidencePresent state)

def applyGrantRecord (state : ReplicaState) (record : TraceRecord) : StepResult :=
  match record.subject, record.capability with
  | some subject, some capability =>
      let nextState := markSeen (grantCapability state subject capability) record.requestId
      { state := nextState, outcome := .acceptedGrant subject capability }
  | _, _ =>
      { state := state, outcome := .invalidShape }

def applyRevokeRecord (state : ReplicaState) (record : TraceRecord) : StepResult :=
  match record.subject, record.capability with
  | some subject, some capability =>
      let nextState := markSeen (revokeCapability state subject capability) record.requestId
      { state := nextState, outcome := .acceptedRevoke subject capability }
  | _, _ =>
      { state := state, outcome := .invalidShape }

def applyEvidenceRecord (state : ReplicaState) (record : TraceRecord) : StepResult :=
  if _hRevoked : hasRevokedPublisher state record.publisher then
    { state := state, outcome := .revokedPublisher }
  else if _hActive : hasActivePublisher state record.publisher then
    match toEvidence? record with
    | some evidence =>
        if _hConflict : conflictingDigest state evidence.source evidence.locator evidence.digest then
          { state := state, outcome := .equivocation }
        else
          let projection := AppliedProjection.evidence evidence
          let nextState := markSeen {
            state with
              evidence := evidence :: state.evidence
              seenLocatorDigests := insertUnique projection.locatorKey state.seenLocatorDigests
          } record.requestId
          { state := nextState, outcome := .acceptedData projection }
    | none =>
        { state := state, outcome := .invalidShape }
  else
    { state := state, outcome := .unauthorized }

def applyMemoryRecord (state : ReplicaState) (record : TraceRecord) : StepResult :=
  if _hRevoked : hasRevokedPublisher state record.publisher then
    { state := state, outcome := .revokedPublisher }
  else if _hActive : hasActivePublisher state record.publisher then
    match toMemory? record with
    | some memory =>
        if _hConflict : conflictingDigest state memory.source memory.locator memory.digest then
          { state := state, outcome := .equivocation }
        else
          if _hEvidence : allEvidencePresent state memory.evidenceRefs then
            if _hVersion : memory.version = currentVersion state memory.canonicalId + 1 then
              let projection := AppliedProjection.memory memory
              let nextState := markSeen {
                state with
                  histories := addVersion state.histories memory.canonicalId memory
                  seenLocatorDigests := insertUnique projection.locatorKey state.seenLocatorDigests
              } record.requestId
              { state := nextState, outcome := .acceptedData projection }
            else
              { state := state, outcome := .versionGap }
          else
            { state := state, outcome := .missingEvidence }
    | none =>
        { state := state, outcome := .invalidShape }
  else
    { state := state, outcome := .unauthorized }

def applyAuthorizedRecord (state : ReplicaState) (record : TraceRecord) : StepResult :=
  match record.kind with
  | .grant => applyGrantRecord state record
  | .revoke => applyRevokeRecord state record
  | .evidence => applyEvidenceRecord state record
  | .memory => applyMemoryRecord state record

def applyRecord (state : ReplicaState) (record : TraceRecord) : StepResult :=
  if _hDuplicate : record.requestId ∈ state.seenRequests then
    { state := state, outcome := .duplicateReplay }
  else if _hAuthority : !authorityValid record then
    { state := state, outcome := .unauthorized }
  else
    applyAuthorizedRecord state record

def replayTrace (state : ReplicaState) : List TraceRecord → TraceRun
  | [] => { finalState := state, outcomes := [] }
  | record :: rest =>
      let step := applyRecord state record
      let tail := replayTrace step.state rest
      { finalState := tail.finalState, outcomes := step.outcome.tag :: tail.outcomes }

def extendRun (run : TraceRun) (record : TraceRecord) : TraceRun :=
  let step := applyRecord run.finalState record
  { finalState := step.state, outcomes := run.outcomes ++ [step.outcome.tag] }

def referenceFoldFrom (run : TraceRun) (records : List TraceRecord) : TraceRun :=
  records.foldl extendRun run

def referenceFold (state : ReplicaState) (records : List TraceRecord) : TraceRun :=
  referenceFoldFrom { finalState := state, outcomes := [] } records

def summarize (state : ReplicaState) : FinalSummary :=
  {
    seenRequestCount := state.seenRequests.length
    evidenceCount := state.evidence.length
    memoryVersionCount := memoryVersionCount state.histories
    activeCanonicalCount := activeCanonicalCount state.histories
    activePublishers := state.activeCapabilities.map (·.principal)
    revokedPublishers := state.revokedCapabilities.map (·.principal)
    canonicalHeads := latestHeads state.histories
  }

end CodifixFederation
