# Client inspection at09:44

East builder e2 uses R-east. Its resolver answer is10.20.0.10. Its process also
keeps an existing HTTP connection to the old mirror; opening no new connection
would not consult DNS again. The client contract permits a connection lifetime
up to20 minutes after connection establishment. The packet lacks the connection's
establishment time, so its exact retirement instant cannot be calculated here.

West builder w1 uses R-west and completed one read from10.20.0.20. A contractor's
VPN client used a public resolver and reached192.0.2.40, not either private
address. That test does not verify the contractor's intended internal DNS view.

The manifest of required segments lists east, west and VPN. It does not list all
individual processes or their resolver/connection caches. Public availability
alone does not close the required three-segment verification.
