import Lake
open Lake DSL

-- Match the root module: the pinned action's leanchecker and nanoda helpers
-- infer it from the package name. No Mathlib or code-extraction dependency.
package CodifixFederation where
  version := v!"0.1.0"

@[default_target]
lean_lib CodifixFederation

@[default_target]
lean_exe codifix_federation where
  root := `Main

@[default_target, test_driver]
lean_exe codifix_federation_tests where
  root := `Tests
