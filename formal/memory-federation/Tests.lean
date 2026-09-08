import MemoryFederation.Trace
import MemoryFederation.PublicationTrace

def main (_args : List String) : IO Unit := do
  MemoryFederation.runDefaultFixtures
  MemoryFederation.Publication.runPublicationFixtures
