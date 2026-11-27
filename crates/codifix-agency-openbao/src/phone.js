"use strict";
const field = id => document.getElementById(id);
let attempt;
async function review() {
  attempt = undefined;
  field("approval").hidden = true;
  const response = await fetch("/unseal/status", { headers: { Authorization: "Bearer " + field("token").value }, cache: "no-store" });
  if (!response.ok) throw Error(response.status === 404 ? "No pending request." : "Unable to verify this custodian.");
  const data = await response.json();
  const expired = data.expires_at <= Date.now() / 1000;
  field("status").textContent = expired && ["waiting", "invalid_share"].includes(data.stage) ? "Request expired." : data.stage.replaceAll("_", " ");
  if ((data.stage === "waiting" || data.stage === "invalid_share") && data.expires_at > Date.now() / 1000) {
    attempt = data.attempt;
    field("details").textContent = "OpenBao is sealed. Instance " + data.installation + " requests the share held by " + data.participant + ". " + data.accepted_before + " of " + data.threshold + " accepted. Attempt " + attempt + ".";
    field("approval").hidden = false;
  }
}
async function decide(decision) {
  if (!attempt) return;
  const payload = { attempt, decision };
  if (decision === "contribute") payload.share = field("share").value;
  field("share").value = "";
  attempt = undefined;
  field("approval").hidden = true;
  const response = await fetch("/unseal/decision", {
    method: "POST", headers: { Authorization: "Bearer " + field("token").value, "Content-Type": "application/json" },
    body: JSON.stringify(payload), cache: "no-store",
  });
  if (response.status === 422) { await review(); return; }
  if (!response.ok) throw Error("Contribution not confirmed. Review the request.");
  await review();
}
function run(action) { action().catch(error => { field("status").textContent = error.message; }); }
field("review").onclick = () => run(review);
field("approve").onclick = () => run(() => decide("contribute"));
field("decline").onclick = () => run(() => decide("decline"));
