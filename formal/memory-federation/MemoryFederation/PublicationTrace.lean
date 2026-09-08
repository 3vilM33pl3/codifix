import MemoryFederation.Publication
import MemoryFederation.Trace

open Lean

namespace MemoryFederation.Publication

structure Summary where
  origin : Nat
  canonical : Nat
  shared : Nat
  «local» : Nat
  revision : Nat
  author : String
  current : Bool
  tombstone : Bool
  deriving Repr, BEq, FromJson

structure Fixture where
  name : String
  records : List Command
  outcomes : List String
  sequence : Nat
  publishers : List Nat
  streams : List Summary
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
    current, tombstone := current && (state.map (·.tombstone)).getD false }

def runPublicationFixtures : IO Unit := do
  for name in ["lifecycle.json", "order-and-ownership.json"] do
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
    for expected in fixture.streams do
      assertEqual s!"{fixture.name}: stream" expected (summarize state expected)
    assertEqual s!"{fixture.name}: replay" state (replay {} fixture.records)
    assertEqual s!"{fixture.name}: fold" state
      (fixture.records.foldl (fun s r => (apply s r).1) {})
    IO.println s!"checked {fixture.name}"

end MemoryFederation.Publication
