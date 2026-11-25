use scheduler_baseline::budget::{
    observe_budgeted, simulate_budgeted, simulate_budgeted_with_limit, BudgetJob, EndState, Report,
};
use scheduler_baseline::{simulate, Job, Policy};
use std::num::NonZeroU32;

fn print_budget_trace(label: &str, report: &Report, jobs: &[BudgetJob]) {
    let choices: Vec<_> = report.dispatches.iter().map(|(_, id)| *id).collect();
    println!("  {label}: dispatches={choices:?}");
    for item in jobs {
        let ticks: Vec<_> = report
            .dispatches
            .iter()
            .filter_map(|(tick, id)| (*id == item.job.id).then_some(*tick))
            .collect();
        let response = ticks[0] - item.job.arrival;
        let longest_wait = ticks
            .windows(2)
            .map(|w| w[1] - w[0] - 1)
            .max()
            .unwrap_or(0)
            .max(response);
        let finished = report
            .outcomes
            .iter()
            .find_map(|outcome| match *outcome {
                EndState::Completed { id, at } if id == item.job.id => Some(at),
                _ => None,
            })
            .expect("example jobs complete");
        println!(
            "    job={} first_wait={} longest_wait={} finished={}",
            item.job.id, response, longest_wait, finished
        );
    }
}

fn main() {
    let workloads: [(&str, &[Job]); 3] = [
        (
            "interactive-and-long",
            &[
                Job {
                    id: 1,
                    arrival: 0,
                    work: 12,
                },
                Job {
                    id: 2,
                    arrival: 2,
                    work: 1,
                },
                Job {
                    id: 3,
                    arrival: 5,
                    work: 1,
                },
            ],
        ),
        (
            "simultaneous-burst",
            &[
                Job {
                    id: 1,
                    arrival: 0,
                    work: 5,
                },
                Job {
                    id: 2,
                    arrival: 0,
                    work: 3,
                },
                Job {
                    id: 3,
                    arrival: 0,
                    work: 1,
                },
            ],
        ),
        (
            "steady-background",
            &[
                Job {
                    id: 1,
                    arrival: 0,
                    work: 6,
                },
                Job {
                    id: 2,
                    arrival: 2,
                    work: 4,
                },
                Job {
                    id: 3,
                    arrival: 4,
                    work: 3,
                },
                Job {
                    id: 4,
                    arrival: 6,
                    work: 2,
                },
            ],
        ),
    ];
    for (name, jobs) in workloads {
        println!("workload={name}");
        for policy in [Policy::RoundRobin, Policy::FirstComeFirstServed] {
            let completed = simulate(jobs, 128, policy).expect("workload fits the horizon");
            let worst_response = completed.iter().map(|job| job.response).max().unwrap_or(0);
            let worst_turnaround = completed
                .iter()
                .map(|job| job.turnaround)
                .max()
                .unwrap_or(0);
            println!(
                "  {policy:?} max_response={worst_response} max_turnaround={worst_turnaround}"
            );
            for job in completed {
                println!(
                    "    job={} response={} turnaround={}",
                    job.id, job.response, job.turnaround
                );
            }
        }
    }

    let jobs = [
        BudgetJob {
            job: Job {
                id: 1,
                arrival: 0,
                work: 5,
            },
            budget: 5,
        },
        BudgetJob {
            job: Job {
                id: 2,
                arrival: 0,
                work: 2,
            },
            budget: 2,
        },
    ];
    let suggestions = [Some(1); 7];
    let unchecked = simulate_budgeted(&jobs, &suggestions, 7).unwrap();
    let limited =
        simulate_budgeted_with_limit(&jobs, &suggestions, 7, NonZeroU32::new(2).unwrap()).unwrap();
    println!("workload=repeated-suggestion (simulated ticks)");
    print_budget_trace("unlimited", &unchecked, &jobs);
    print_budget_trace("limit-2", &limited, &jobs);

    println!("workload=partial-horizon (four simulated ticks)");
    for (name, recommendations, limit) in [
        ("unlimited", &suggestions[..4], None),
        ("limit-2", &suggestions[..4], NonZeroU32::new(2)),
        ("round-robin", &suggestions[..0], None),
    ] {
        let observation = observe_budgeted(&jobs, recommendations, 4, limit).unwrap();
        let choices: Vec<_> = observation
            .report
            .dispatches
            .iter()
            .map(|(_, id)| *id)
            .collect();
        println!(
            "  {name}: dispatches={choices:?} complete={}",
            observation.complete
        );
        for item in observation.jobs {
            println!(
                "    job={} state={:?} first_wait={:?} longest_wait={:?} waiting_at_end={:?}",
                item.id, item.state, item.first_wait, item.longest_wait, item.waiting_at_end
            );
        }
    }
}
