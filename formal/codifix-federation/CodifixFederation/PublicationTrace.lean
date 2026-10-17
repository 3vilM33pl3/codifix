import CodifixFederation.Publication
import CodifixFederation.Trace

open Lean

namespace CodifixFederation.Publication

structure Summary where
  origin : Nat
  canonical : Nat
  shared : Nat
  «local» : Nat
  revision : Nat
  author : String
  current : Bool
  tombstone : Bool
  text : String
  evidence : List Key
  deriving Repr, BEq, FromJson

structure EvidenceExpected where
  publisher : Nat
  id : Nat
  text : String
  deriving FromJson

structure ProposalExpected where
  publisher : Nat
  id : Nat
  status : String
  version : Nat
  deriving FromJson

structure RelationExpected where
  publisher : Nat
  id : Nat
  revisions : List Nat
  active : List Bool
  deriving FromJson

structure Fixture where
  name : String
  records : List Command
  outcomes : List String
  sequence : Nat
  publishers : List Nat
  pullers : List Nat
  managers : List Nat
  streams : List Summary
  evidence : List EvidenceExpected
  proposals : List ProposalExpected
  relations : List RelationExpected
  deriving FromJson

def summarize (s : State) (expected : Summary) : Summary :=
  let memory := latest s .memory expected.origin expected.canonical
  let state := latest s .state expected.origin expected.canonical
  let current := state.isSome && state.map (·.shared) == memory.map (·.shared)
  { origin := expected.origin, canonical := expected.canonical,
    shared := (memory.map (·.shared)).getD 0,
    «local» := (memory.map (·.local)).getD 0,
    revision := (state.map (·.revision)).getD 0,
    author := (memory.map (·.author)).getD "",
    current, tombstone := current && (state.map (·.tombstone)).getD false,
    text := (memory.map (·.text)).getD "", evidence := (memory.map (·.evidence)).getD [] }

def runPublicationFixtures : IO Unit := do
  for name in ["lifecycle.json", "order-and-ownership.json", "evidence-and-proposals.json",
      "relations-and-causation.json", "capabilities.json", "expertise.json"] do
    let raw ← IO.FS.readFile (System.FilePath.mk "../../fixtures/federation/publication-traces" / name)
    let json ← IO.ofExcept (Json.parse raw)
    let fixture : Fixture ← IO.ofExcept (fromJson? json)
    let mut state : State := {}
    let mut outcomes := []
    for r in fixture.records do
      let (next, outcome) := apply state r
      if outcome != "applied" then
        assertEqual s!"{fixture.name}: rejected/duplicate state changed" state next
      state := next
      outcomes := outcomes ++ [outcome]
    assertEqual s!"{fixture.name}: outcomes" fixture.outcomes outcomes
    assertEqual s!"{fixture.name}: sequence" fixture.sequence state.sequence
    assertEqual s!"{fixture.name}: publishers" fixture.publishers (state.publishers.mergeSort (· ≤ ·))
    assertEqual s!"{fixture.name}: pullers" fixture.pullers (state.pullers.mergeSort (· ≤ ·))
    assertEqual s!"{fixture.name}: managers" fixture.managers (state.managers.mergeSort (· ≤ ·))
    for expected in fixture.streams do
      assertEqual s!"{fixture.name}: stream" expected (summarize state expected)
    for expected in fixture.evidence do
      let e := byKey state ⟨expected.publisher, expected.id⟩
      assertEqual s!"{fixture.name}: evidence kind" (some Kind.evidence) (e.map (·.kind))
      assertEqual s!"{fixture.name}: evidence text" (some expected.text) (e.map (·.text))
    for expected in fixture.proposals do
      let k := some (Key.mk expected.publisher expected.id)
      assertEqual s!"{fixture.name}: proposal exists" true (proposalRecord state k).isSome
      let resolved := resolution state k
      let status := match resolved with
        | none => "pending"
        | some r => if r.local = 0 then "rejected" else "accepted"
      let version := match resolved with
        | none => 0
        | some r => ((state.versions.find? (fun v => key v == key r)).map (·.shared)).getD 0
      assertEqual s!"{fixture.name}: proposal status" expected.status status
      assertEqual s!"{fixture.name}: approval version" expected.version version
    for expected in fixture.relations do
      let history := (state.accepted.filter fun r =>
        r.kind == .relation && r.publisher == expected.publisher && r.relationId == expected.id).reverse
      assertEqual s!"{fixture.name}: relation revisions" expected.revisions (history.map (·.revision))
      assertEqual s!"{fixture.name}: relation activity" expected.active (history.map (·.active))
    assertEqual s!"{fixture.name}: replay" state (replay {} fixture.records)
    assertEqual s!"{fixture.name}: fold" state
      (fixture.records.foldl (fun s r => (apply s r).1) {})
    IO.println s!"checked {fixture.name}"

end CodifixFederation.Publication
