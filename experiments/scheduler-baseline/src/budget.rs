//! Finite host-only model of budget depletion and untrusted policy suggestions.
use std::collections::VecDeque;

use crate::Job;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetJob {
    pub job: Job,
    /// Total lifetime CPU ticks in this experiment; budgets do not replenish.
    pub budget: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndState {
    Completed { id: u32, at: u32 },
    BudgetExhausted { id: u32, at: u32, remaining: u32 },
}

#[derive(Debug, Eq, PartialEq)]
pub struct Report {
    pub dispatches: Vec<(u32, u32)>,
    pub outcomes: Vec<EndState>,
    pub fallback_dispatches: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub enum BudgetError {
    InvalidWork,
    DuplicateId,
    TooManyJobs,
    HorizonExceeded,
}

/// A recommendation is a job ID indexed by simulated tick. Missing, invalid,
/// unavailable, or exhausted IDs use the round-robin queue instead. It is a
/// pre-supplied trace, not a live policy server or a time-based failure model.
pub fn simulate_budgeted(
    jobs: &[BudgetJob],
    recommendations: &[Option<u32>],
    horizon: u32,
) -> Result<Report, BudgetError> {
    if jobs.len() > 1024 {
        return Err(BudgetError::TooManyJobs);
    }
    for (i, item) in jobs.iter().enumerate() {
        if item.job.work == 0 {
            return Err(BudgetError::InvalidWork);
        }
        if jobs[..i].iter().any(|other| other.job.id == item.job.id) {
            return Err(BudgetError::DuplicateId);
        }
    }
    let mut remaining: Vec<_> = jobs.iter().map(|item| item.job.work).collect();
    let mut budget: Vec<_> = jobs.iter().map(|item| item.budget).collect();
    let mut queue = VecDeque::new();
    let mut report = Report {
        dispatches: Vec::new(),
        outcomes: Vec::new(),
        fallback_dispatches: 0,
    };
    for tick in 0..horizon {
        for (i, item) in jobs.iter().enumerate() {
            if item.job.arrival == tick {
                if budget[i] == 0 {
                    report.outcomes.push(EndState::BudgetExhausted {
                        id: item.job.id,
                        at: tick,
                        remaining: remaining[i],
                    });
                } else {
                    queue.push_back(i);
                }
            }
        }
        if !queue.is_empty() {
            let requested = recommendations.get(tick as usize).copied().flatten();
            let position =
                requested.and_then(|id| queue.iter().position(|i| jobs[*i].job.id == id));
            let i = match position {
                Some(pos) => queue.remove(pos).expect("position belongs to queue"),
                None => {
                    report.fallback_dispatches += 1;
                    queue.pop_front().expect("queue is nonempty")
                }
            };
            report.dispatches.push((tick, jobs[i].job.id));
            remaining[i] -= 1;
            budget[i] -= 1;
            if remaining[i] == 0 {
                report.outcomes.push(EndState::Completed {
                    id: jobs[i].job.id,
                    at: tick + 1,
                });
            } else if budget[i] == 0 {
                report.outcomes.push(EndState::BudgetExhausted {
                    id: jobs[i].job.id,
                    at: tick + 1,
                    remaining: remaining[i],
                });
            } else {
                queue.push_back(i);
            }
        }
        if report.outcomes.len() == jobs.len() {
            return Ok(report);
        }
    }
    if jobs.is_empty() {
        Ok(report)
    } else {
        Err(BudgetError::HorizonExceeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job(id: u32, work: u32, budget: u32) -> BudgetJob {
        BudgetJob {
            job: Job {
                id,
                arrival: 0,
                work,
            },
            budget,
        }
    }
    #[test]
    fn exhaustion_stops_job_and_others_continue() {
        let report = simulate_budgeted(&[job(1, 4, 1), job(2, 2, 2)], &[], 3).unwrap();
        assert_eq!(report.dispatches, [(0, 1), (1, 2), (2, 2)]);
        assert_eq!(
            report.outcomes,
            [
                EndState::BudgetExhausted {
                    id: 1,
                    at: 1,
                    remaining: 3
                },
                EndState::Completed { id: 2, at: 3 }
            ]
        );
        assert_eq!(report.fallback_dispatches, 3);
    }
    #[test]
    fn invalid_missing_and_exhausted_suggestions_fall_back() {
        let report = simulate_budgeted(
            &[job(1, 2, 1), job(2, 2, 2)],
            &[Some(999), Some(1), None],
            3,
        )
        .unwrap();
        assert_eq!(report.dispatches, [(0, 1), (1, 2), (2, 2)]);
        assert_eq!(report.fallback_dispatches, 3);
    }
    #[test]
    fn valid_suggestion_never_bypasses_budget() {
        let report =
            simulate_budgeted(&[job(1, 2, 1), job(2, 1, 1)], &[Some(1), Some(1)], 2).unwrap();
        assert_eq!(report.dispatches, [(0, 1), (1, 2)]);
        assert_eq!(report.fallback_dispatches, 1);
    }
    #[test]
    fn zero_budget_and_horizon_are_explicit() {
        let report = simulate_budgeted(&[job(1, 1, 0), job(2, 1, 1)], &[], 1).unwrap();
        assert_eq!(
            report.outcomes[0],
            EndState::BudgetExhausted {
                id: 1,
                at: 0,
                remaining: 1
            }
        );
        assert_eq!(
            simulate_budgeted(&[job(1, 2, 2)], &[], 1),
            Err(BudgetError::HorizonExceeded)
        );
    }
}
