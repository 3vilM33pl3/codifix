# Birch job service, incident 118

At 09:10 UTC the API scaled from four to twelve replicas after request latency
rose. By 09:14, 28% of sampled requests timed out waiting for a database pool
connection. No SQL statement had started for those timeouts. Database CPU was
34%; storage read latency in the sampled minute was unchanged.

At 09:16 the database had 192 job-service sessions and eight operations sessions.
Twenty-four active job-service transactions were waiting on the same audit-row
lock held by a long reporting transaction. The others included idle and runnable
sessions; the sample does not classify each one. The reporter began at 09:08.

The API's cached liveness route stayed green because it does not check the pool.
At 09:20 admission was temporarily reduced. Pool waiters fell in the next five
minutes while the reporter also ended. There was no controlled experiment
separating those two changes, nor a measurement of the maximum sustainable rate.
