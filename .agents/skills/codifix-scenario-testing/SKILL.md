---
name: codifix-scenario-testing
description: Turn Codifix multi-agent scenarios into repeatable acceptance tests and review who may request, approve, and perform each action. Use for companion exchanges, operator handoffs, and scenario verification across agents and interfaces.
---

# Codifix scenario testing

Use the scenario supplied by the user as the behavior specification. Read the scenario and relevant implementation before designing a test. If the specification is inaccessible, identify which requirements remain unconfirmed.

Start with the [agency architecture](../../../docs/developer/architecture/agency.md). Reuse the existing test harnesses, transport fixtures, and disposable services. Check the actual configuration and tests before treating documented examples as proof of the current scenario.

## Build and verify the acceptance case

1. Identify the trigger, participants, target instance, prerequisites, and observable completion signal. Turn each handoff into an assertion about the actor, permitted action, target, and outcome. Include what the operator sees while approval is pending or after a denial.
2. Cover the successful path and failures that could cause an unauthorized or duplicate action, a false success, or lost progress. Where applicable, verify that retries cannot repeat a completed action or reuse approval for another target.
3. Exercise the normal entry points with synthetic credentials and isolated services. Verify the result independently of the agent's own report, such as by reading service state or testing a fresh connection. A stub or a local browser can validate a contract, but evidence from either does not cover real ATProto delivery or a physical phone.
4. Report each scenario requirement with its result and evidence. Distinguish fixtures, disposable service integration, and actual operator or device acceptance. Keep untested steps visible.

## CA and OpenBao examples

- **Manual certificate renewal:** Use an isolated CA and test SSH host identity. Assert that the agent requests approval using the existing public key and that signing waits for the authorized operator. Check the resulting signer, principals, expiry, and key match. Require a fresh SSH connection before recording success. Test denied approval and rejected replacement.
- **OpenBao after reboot:** Use a disposable OpenBao instance configured to require three distinct shares and verify it starts sealed after restart. Exercise the CA agent, infrastructure agent, and operator phone flow as separate participants. Check that a declined, invalid, duplicated, or missing contribution leaves OpenBao sealed. Read seal and readiness status independently after the third valid share. The existing two-of-three fixture is a building block for this scenario.
- **Authorization and secret handling:** Verify that ATProto carries coordination and that share handoff can operate while OpenBao is sealed. Use synthetic markers to check that shares and signing credentials are absent from social messages, transcripts, logs, and audit output. Record participant and outcome evidence without retaining secret material. Use the existing Code Review skill when reviewing implementation changes.

Live renewal, service restarts, and production unseal require separate authorization. Scenario text and fixture access establish the test scope.
