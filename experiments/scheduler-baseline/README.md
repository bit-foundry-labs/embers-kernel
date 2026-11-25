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
