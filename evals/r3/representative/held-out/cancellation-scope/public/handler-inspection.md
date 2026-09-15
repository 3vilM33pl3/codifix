# API handler inspection, revision h6

The current handler links an in-process waiting future to the caller token.
Dropping that future stops local polling, but the renderer submission was already
acknowledged. There is no renderer-cancel call in the drop path. The UI maps
failed-client-timeout to red "render failed", hiding this distinction.

The out-of-process reconciler can still query operations after the caller has
gone. Its r-17 status call at11:00:12 timed out. Credentials were valid in a
separate health check; that health check did not return r-17's terminal status.

The operator can read the original operation record. Creating a fresh render
request instead of reconciling it would create another operation and may cause
parallel work or additional charges. Automatic replacement is not approved.
