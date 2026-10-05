import { afterEach, expect, it } from "vitest";
import { parseCompanyAgentEnvelope, validAgentReceipt, matchesAgentRequest } from "./companyAgentTypes";
import { consumeAgentRequestLink } from "./agentRequestLink";
import { aid, agentEnvelope, agentReceipt } from "./companyAgentFixtures.testdata";
afterEach(() => window.history.replaceState(null, "", "/"));
it("accepts exact public metadata, including same project UUID on different owners", () => {
  const value = agentEnvelope(); expect(parseCompanyAgentEnvelope(JSON.stringify(value))).toEqual(value);
  delete value.automatic_approval; expect(parseCompanyAgentEnvelope(JSON.stringify(value))).toEqual(value);
  const receipt = agentReceipt(value); expect(validAgentReceipt(receipt, aid(2), aid(1))).toBe(true); expect(matchesAgentRequest(receipt, value.request)).toBe(true);
  receipt.projects[1].selection.manifest_revision++; expect(matchesAgentRequest(receipt, value.request)).toBe(false);
});
it.each(["secret", "automatic", "endpoint", "trust", "duplicate", "provider", "nil", "oversized"])("rejects %s input before any server request", variant => {
  const value = agentEnvelope() as any;
  if (variant === "secret") value.request.token = "secret";
  if (variant === "automatic") value.automatic_approval = true;
  if (variant === "endpoint") value.approval_endpoint = "https://other.test/agents";
  if (variant === "trust") value.request.projects[0].area = "trust";
  if (variant === "duplicate") value.request.projects[1] = value.request.projects[0];
  if (variant === "provider") value.request.provider_id = "anthropic";
  if (variant === "nil") value.request.request_id = "00000000-0000-0000-0000-000000000000";
  expect(() => parseCompanyAgentEnvelope(variant === "oversized" ? " ".repeat(65537) : JSON.stringify(value))).toThrow("Invalid public agent envelope");
});
it("removes public link query immediately while preserving unrelated URL state; no auth-return storage", () => {
  const raw = JSON.stringify(agentEnvelope()); window.history.replaceState(null, "", `/?agent_request=${encodeURIComponent(raw)}&other=1#hello`);
  expect(consumeAgentRequestLink(window.location, window.history).request).toBe(raw); expect(window.location.search).toBe("?other=1"); expect(window.location.hash).toBe("#hello");
});
it.each(["x".repeat(8193), "{bad", JSON.stringify({ ...agentEnvelope(), contains_secret: true })])("clears invalid/oversized links without retaining payload", raw => {
  window.history.replaceState(null, "", `/?agent_request=${encodeURIComponent(raw)}`);
  expect(consumeAgentRequestLink(window.location, window.history)).toEqual({ notice: expect.stringContaining("Invalid or oversized") }); expect(window.location.search).toBe("");
});
