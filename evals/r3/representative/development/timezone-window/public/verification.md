# Scheduler review checklist

Verify the date-to-instant resolution against Z17 for ordinary days, both sides
of the autumn fold and the spring gap. Confirm first-occurrence selection and a
visible skip reason. Check restart/catch-up against schedule/date receipts; a
new job ID must not authorize a second dispatch of a completed occurrence.

Changing the rule-table version requires reviewed future resolutions. Preserve
past receipts; do not reinterpret them as unexecuted work. Compare intended civil
date, resolved instant, table version and outcome separately in the audit view.

The rehearsal demonstrated duplicate rejection at the fake transport, not an
exactly-once guarantee across every production provider. It supplies no evidence
of how many customers an older release affected, no annual miss rate, and no
promise about future real-world zone legislation.
