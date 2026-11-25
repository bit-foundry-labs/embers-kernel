# Deterministic scheduler baseline

This host-only Rust experiment models one CPU with one-tick round-robin dispatch and a non-preemptive first-come-first-served comparison. It is an independent starting point for issue #4; it does not implement a kernel scheduler, hardware timing, enforcement, or AI policy. The existing policy-guard experiment models admission, not execution, and is not connected here.

At each tick, jobs arriving at that tick enter the queue in input order. One queued job runs for one tick and rejoins the tail if incomplete. A job arriving at the next tick follows jobs already in the queue. Under first-come-first-served, the active job remains at the front until completion; equal-arrival jobs retain input order. Completion is measured at the end of a tick; response is first-run time minus arrival, and turnaround is finish time minus arrival. Idle ticks advance time. Empty jobs, duplicate IDs, more than 1,024 jobs, and runs that do not finish within the caller's horizon are rejected. No partial measurements are returned on a horizon failure.

```sh
cargo test --offline --manifest-path experiments/scheduler-baseline/Cargo.toml
cargo run --offline --manifest-path experiments/scheduler-baseline/Cargo.toml
cargo clippy --offline --all-targets --manifest-path experiments/scheduler-baseline/Cargo.toml -- -D warnings
```

The example includes two initial bursts and a later arrival. Units are simulated ticks, not milliseconds or performance measurements. This initial model does not measure context switching overhead, I/O blocking, deadlines, budgets, starvation over unbounded workloads, multicore contention, energy, or p99 latency. The example compares two policies on one hand-authored trace; it is not enough to establish which policy performs better generally. Future experiments should add representative, reproducible traces and aggregate measurements.

The executable runs three fixed traces: interactive bursts alongside a long job, simultaneous arrivals, and staggered background jobs. It reports each job's response and turnaround in simulated ticks plus the maximum of each per trace and policy. Maximum response is the longest wait for the first CPU tick, not the longest gap between subsequent ticks. These deliberately small synthetic examples are not representative performance benchmarks.

The separate `budget` module models a finite lifetime CPU allowance per job. Once exhausted, the job stops with its unfinished work reported; budgets never replenish. Each tick may carry a pre-supplied job-ID recommendation. A missing or invalid recommendation falls back to the round-robin queue; a recommendation can select only a currently runnable, budgeted job. This is a sequential host model with a trusted simulator and supplied recommendations, not an authenticated capability, real scheduler, actual policy process, timed deadline, or live failure recovery. In particular, valid recommendations can repeatedly favor one job, so no fairness or starvation guarantee is claimed. The caller supplies a finite horizon; incomplete runs return an error.

The optional `simulate_budgeted_with_limit` experiment allows at most a nonzero number of consecutive accepted recommendations before selecting the front of the round-robin queue. Any queue selection or an idle tick resets the count; the default `simulate_budgeted` remains unchanged. The limit counts all accepted recommendations, even when recommendations alternate between jobs. Both paths respect finite job budgets. `forced_fallback_dispatches` counts queue selections caused by this limit, including selections when only one job is runnable.

The executable also prints a small repeated-suggestion comparison. For each job, `first_wait` counts ticks from arrival until first dispatch; `longest_wait` is the maximum of that first wait and the number of non-running ticks between its consecutive dispatches. This example has enough budget for every job to finish. It does not measure post-exhaustion waiting or model jobs that block. These trace-specific figures are simulated ticks, not wall-clock measurements, a general fairness bound, or a guarantee for a real scheduler.
