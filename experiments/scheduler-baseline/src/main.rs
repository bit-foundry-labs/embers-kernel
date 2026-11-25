use scheduler_baseline::{simulate, Job, Policy};

fn main() {
    let jobs = [
        Job {
            id: 1,
            arrival: 0,
            work: 2,
        },
        Job {
            id: 2,
            arrival: 0,
            work: 6,
        },
        Job {
            id: 3,
            arrival: 3,
            work: 2,
        },
    ];
    for policy in [Policy::RoundRobin, Policy::FirstComeFirstServed] {
        println!("{policy:?}");
        for job in simulate(&jobs, 16, policy).expect("example workload fits the horizon") {
            println!(
                "  job={} first={} finish={} response={} turnaround={}",
                job.id, job.first_run, job.finished, job.response, job.turnaround
            );
        }
    }
}
