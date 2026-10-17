import CodifixFederation.Trace
import CodifixFederation.PublicationTrace

def main (_args : List String) : IO Unit := do
  CodifixFederation.runDefaultFixtures
  CodifixFederation.Publication.runPublicationFixtures
