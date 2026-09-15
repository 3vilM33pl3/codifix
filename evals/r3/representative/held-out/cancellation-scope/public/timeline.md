# Operation r-17 timeline

11:00:00 API stores r-17 and receives renderer acknowledgement accepted.
11:00:02 renderer status changes to running.
11:00:05 caller's five-second wait expires; API connection closes with timeout.
11:00:06 local handler writes failed-client-timeout into the old UI status field.
11:00:09 operator submits explicit cancellation for r-17.
11:00:10 renderer replies stop-requested.
11:00:12 renderer status request times out; no terminal receipt is obtained.

The last confirmed renderer state before the stop request was running. There is
no completion or canceled receipt in this packet. The timeout response seen by
the caller is not an independent observation of renderer termination.

A separate test r-18 was canceled before dispatch and has a terminal
canceled-before-start receipt. r-18 is not evidence that r-17 followed that path.
