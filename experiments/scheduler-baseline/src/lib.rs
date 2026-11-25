#![forbid(unsafe_code)]

//! Deterministic, single-CPU scheduling experiment. This is not kernel code.
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Job {
    pub id: u32,
    pub arrival: u32,
    pub work: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Completion {
    pub id: u32,
    pub first_run: u32,
    pub finished: u32,
    pub response: u32,
    pub turnaround: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    EmptyWork,
    DuplicateId,
    HorizonExceeded,
    TooManyJobs,
}

/// Simulate one tick per dispatch, admitting arrivals before dispatch in input order.
/// The bounded horizon includes idle ticks; unfinished jobs return an error.
pub fn round_robin(jobs: &[Job], horizon: u32) -> Result<Vec<Completion>, Error> {
    if jobs.len() > 1024 {
        return Err(Error::TooManyJobs);
    }
    for (i, job) in jobs.iter().enumerate() {
        if job.work == 0 {
            return Err(Error::EmptyWork);
        }
        if jobs[..i].iter().any(|other| other.id == job.id) {
            return Err(Error::DuplicateId);
        }
    }
    let mut remaining: Vec<_> = jobs.iter().map(|job| job.work).collect();
    let mut first = vec![None; jobs.len()];
    let mut ready = VecDeque::new();
    let mut completed = Vec::new();
    for time in 0..horizon {
        for (i, job) in jobs.iter().enumerate() {
            if job.arrival == time {
                ready.push_back(i);
            }
        }
        if let Some(i) = ready.pop_front() {
            let start = *first[i].get_or_insert(time);
            remaining[i] -= 1;
            if remaining[i] == 0 {
                let finished = time + 1;
                completed.push(Completion {
                    id: jobs[i].id,
                    first_run: start,
                    finished,
                    response: start - jobs[i].arrival,
                    turnaround: finished - jobs[i].arrival,
                });
            } else {
                ready.push_back(i);
            }
        }
        if completed.len() == jobs.len() {
            return Ok(completed);
        }
    }
    if jobs.is_empty() {
        Ok(completed)
    } else {
        Err(Error::HorizonExceeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interleaves_and_measures() {
        let jobs = [
            Job {
                id: 1,
                arrival: 0,
                work: 2,
            },
            Job {
                id: 2,
                arrival: 0,
                work: 1,
            },
        ];
        assert_eq!(
            round_robin(&jobs, 3).unwrap(),
            vec![
                Completion {
                    id: 2,
                    first_run: 1,
                    finished: 2,
                    response: 1,
                    turnaround: 2
                },
                Completion {
                    id: 1,
                    first_run: 0,
                    finished: 3,
                    response: 0,
                    turnaround: 3
                },
            ]
        );
    }
    #[test]
    fn handles_idle_and_boundary() {
        let jobs = [Job {
            id: 7,
            arrival: 2,
            work: 1,
        }];
        assert_eq!(round_robin(&jobs, 2), Err(Error::HorizonExceeded));
        assert_eq!(round_robin(&jobs, 3).unwrap()[0].finished, 3);
        assert!(round_robin(&[], 0).unwrap().is_empty());
    }
    #[test]
    fn rejects_invalid_inputs() {
        let job = Job {
            id: 1,
            arrival: 0,
            work: 1,
        };
        assert_eq!(round_robin(&[job, job], 3), Err(Error::DuplicateId));
        assert_eq!(
            round_robin(&[Job { work: 0, ..job }], 3),
            Err(Error::EmptyWork)
        );
        assert_eq!(round_robin(&vec![job; 1025], 3), Err(Error::TooManyJobs));
    }
}
