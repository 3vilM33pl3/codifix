import MemoryFederation.Proofs

open Lean

namespace MemoryFederation

def parseFixture (path : System.FilePath) : IO TraceFixture := do
  let raw ← IO.FS.readFile path
  let json ←
    match Json.parse raw with
    | Except.ok value => pure value
    | Except.error err => throw <| IO.userError s!"failed to parse JSON in {path}: {err}"
  match fromJson? json with
  | Except.ok fixture => pure fixture
  | Except.error err => throw <| IO.userError s!"failed to decode fixture {path}: {err}"

def defaultTraceDir : System.FilePath :=
  System.FilePath.mk "../../fixtures/federation/traces"

def defaultFixtureNames : List String :=
  [
    "happy-path.json",
    "duplicate-replay.json",
    "equivocation.json",
    "version-gap.json",
    "missing-evidence.json",
    "tombstone.json",
    "post-revocation.json",
    "publisher-author.json",
    "forged-publisher.json"
  ]

def fixturePath (traceDir : System.FilePath) (name : String) : System.FilePath :=
  traceDir / name

def assertEqual [BEq α] [Repr α] (label : String) (expected actual : α) : IO Unit := do
  if expected == actual then
    pure ()
  else
    throw <| IO.userError s!"{label}\nexpected: {repr expected}\nactual:   {repr actual}"

def runFixture (fixture : TraceFixture) : IO Unit := do
  let replay := replayTrace default fixture.records
  let folded := referenceFold default fixture.records
  let summary := summarize replay.finalState
  assertEqual s!"reference fold mismatch for {fixture.name}" folded replay
  assertEqual s!"outcomes mismatch for {fixture.name}" fixture.expected.outcomes replay.outcomes
  assertEqual s!"summary mismatch for {fixture.name}" fixture.expected.summary summary

def runFixturesFrom (traceDir : System.FilePath) (names : List String) : IO Unit := do
  for name in names do
    let path := fixturePath traceDir name
    let fixture ← parseFixture path
    runFixture fixture
    IO.println s!"checked {fixture.name}"

def runDefaultFixtures : IO Unit :=
  runFixturesFrom defaultTraceDir defaultFixtureNames

end MemoryFederation
