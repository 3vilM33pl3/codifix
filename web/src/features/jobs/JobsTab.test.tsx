import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { jobsRequest } from "../../api";
import { JobsTab } from "./JobsTab";
vi.mock("../../api", () => ({ jobsRequest: vi.fn() }));
const job = { id: "job", project_id: "a", operation_id: "operation", kind: "embedding", status: "blocked", generation: 1, attempts: 1, available_at: "2026-09-10T00:00:00Z", last_failure: "credentials" };
const page = { jobs: [job], next_after: null, stats: { queued: 0, running: 0, retry_wait: 0, blocked: 1, failed: 0, oldest_ready_age_seconds: null }, worker: { enabled: true, last_poll_at: null } };
const detail = { job, stages: [job], dependencies: [], history: [{ generation: 1, attempt: 1, outcome: "blocked", failure: "credentials", uncertain: true, started_at: job.available_at, finished_at: job.available_at }] };
afterEach(cleanup);
beforeEach(() => { vi.mocked(jobsRequest).mockReset(); });
it("confirms retries and retains request identity after an uncertain response", async () => {
  let tries = 0;
  vi.mocked(jobsRequest).mockImplementation(async (_project, suffix) => {
    if (suffix?.startsWith("?")) return page;
    if (suffix === "/job") return detail;
    if (++tries === 1) throw new Error("503 uncertain");
    return {};
  });
  render(<JobsTab project="a" readOnly={false} />);
  fireEvent.click(await screen.findByText("Inspect job"));
  fireEvent.click(await screen.findByText("Review retry"));
  expect(screen.getByRole("button", { name: "Confirm retry" })).toBeDisabled();
  fireEvent.click(screen.getByLabelText("I confirm this retry."));
  fireEvent.click(screen.getByRole("button", { name: "Confirm retry" }));
  await screen.findByRole("alert");
  await waitFor(() => expect(screen.getByRole("button", { name: "Confirm retry" })).not.toBeDisabled());
  fireEvent.click(screen.getByRole("button", { name: "Confirm retry" }));
  await screen.findByText("Retry accepted. Execution remains subject to current policy.");
  const retries = vi.mocked(jobsRequest).mock.calls.filter(c => c[1] === "/job/retry");
  expect(retries).toHaveLength(2);
  expect(retries[0][2]).toEqual(retries[1][2]);
  expect(Object.keys(retries[0][2] as object)).toEqual(["request_id"]);
});
it("clears project-local details and confirmations and aborts prior reads", async () => {
  vi.mocked(jobsRequest).mockImplementation(async (_project, suffix) => suffix === "/job" ? detail : page);
  const { rerender } = render(<JobsTab project="a" readOnly={false} />);
  fireEvent.click(await screen.findByText("Inspect job"));
  fireEvent.click(await screen.findByText("Review retry"));
  rerender(<JobsTab project="b" readOnly={false} />);
  expect(screen.queryByLabelText("Confirm job retry")).toBeNull();
  expect(screen.queryByLabelText("Job details")).toBeNull();
  expect(vi.mocked(jobsRequest).mock.calls[0][3]?.aborted).toBe(true);
});
it("disables retry on read-only services", async () => {
  vi.mocked(jobsRequest).mockImplementation(async (_project, suffix) => suffix === "/job" ? detail : page);
  render(<JobsTab project="a" readOnly />);
  fireEvent.click(await screen.findByText("Inspect job"));
  expect(await screen.findByText("Review retry")).toBeDisabled();
});
it.each([["401", "Signed out"], ["403", "Forbidden"], ["503", "Offline"]])("distinguishes %s from an empty queue", async (code, text) => {
  vi.mocked(jobsRequest).mockRejectedValue(new Error(`${code} failure`));
  render(<JobsTab project="a" readOnly={false} />);
  expect(await screen.findByRole("alert")).toHaveTextContent(text);
  expect(screen.queryByText("No jobs match this page and filter.")).toBeNull();
});
