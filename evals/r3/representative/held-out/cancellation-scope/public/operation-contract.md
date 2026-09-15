# Rowan render submission contract R8

The API accepts a render request, durably records operation identity and submits
work to a remote renderer. Client connection lifetime is not operation lifetime.
An HTTP timeout or disconnected caller cancels waiting for a response; it does
not prove the renderer stopped or the accepted work rolled back.

Remote cancellation is an explicit command addressed to the original operation.
Its acknowledgement has one of three meanings: canceled-before-start means no
render started; stop-requested means a running renderer will attempt to stop;
already-completed means cancellation did not undo the completed render. Only a
confirmed terminal renderer receipt establishes the final operation outcome.

Local state distinguishes accepted, running, stop-requested, completed, canceled
and uncertain. A cancel request is not a completion receipt. Reconciliation keeps
operation identity and never turns an absent reply into an invented terminal state.
Billing follows actual renderer work under a separate contract, not HTTP status.
