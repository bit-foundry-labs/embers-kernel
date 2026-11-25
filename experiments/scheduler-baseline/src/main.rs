use scheduler_baseline::{simulate, Job, Policy};

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
}
