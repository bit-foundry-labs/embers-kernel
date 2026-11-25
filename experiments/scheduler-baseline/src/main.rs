use scheduler_baseline::{round_robin, Job};

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
    for job in round_robin(&jobs, 16).expect("example workload fits the horizon") {
        println!(
            "job={} first={} finish={} response={} turnaround={}",
            job.id, job.first_run, job.finished, job.response, job.turnaround
        );
    }
}
