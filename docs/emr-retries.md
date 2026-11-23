# EMR retry queue

An administrator marking a sync as failed schedules it for a retry after 60 seconds. Each claimed retry doubles the delay until the fifth attempt, after which the exchange remains failed for manual review. The queue holds up to 64 entries. If it is full, process due entries and retry the status update.

An authorized off-chain worker calls process_retries to claim due IDs, performs the sync, then records the resulting status. Claiming marks an exchange in progress to prevent duplicate claims. If a worker stops after claiming an exchange, an administrator can mark it failed again to schedule the next attempt.
