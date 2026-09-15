# Cleanup attempt K31

At 12:05 the worker skipped a10 with reason active_hold:H4. It requested object
deletion for a11; the storage request timed out before acknowledgement. The
worker recorded outcome uncertain, not deleted. No follow-up object existence
check is present. a12 was not eligible and was not requested for deletion.

The earlier paused worker did not delay search expiry: reads check eligibility
at access time. A cached search entry must also recheck eligibility before
returning bytes. Cleanup does not emit attachment bytes into its status log.

Reconciliation must inspect the exact a11 object generation and update the
confirmed result. Reusing the original deletion intent is allowed; broad prefix
deletion or recording success from timeout alone is not. This journal does not
contain a later confirmed cleanup outcome.
