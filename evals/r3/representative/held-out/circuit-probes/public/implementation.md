# Guard implementation inspection

Each API replica keeps its own half-open boolean. The configured shared guard
store contains generation/cooldown but is not consulted atomically when claiming
a probe. Completion handling uses only success/failure and does not compare the
stored generation or claim identity. These two omissions are separate defects.

The public health dashboard reads cached liveness. Its green state says the API
process is responding, not that a current dependency probe succeeded. Increasing
the timeout or cooldown would change timing but not create shared claims or fence
stale completions.

The shared store supports compare-and-swap transitions and holds current state.
The packet describes that capability, not evidence the missing checks have been
deployed. No completed rollout of a corrected guard is recorded.
