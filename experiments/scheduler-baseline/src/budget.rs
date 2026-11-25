//! Finite host-only model of budget depletion and untrusted policy suggestions.
use std::collections::VecDeque;
use std::num::NonZeroU32;

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
    /// Fallbacks specifically caused by the recommendation limit.
    pub forced_fallback_dispatches: u32,
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
    simulate_with_limit(jobs, recommendations, horizon, None)
}

/// Permit at most `limit` recommended dispatches between queue selections.
/// This is a sequential experiment, not a real-time or kernel guarantee.
pub fn simulate_budgeted_with_limit(
    jobs: &[BudgetJob],
    recommendations: &[Option<u32>],
    horizon: u32,
    limit: NonZeroU32,
) -> Result<Report, BudgetError> {
    simulate_with_limit(jobs, recommendations, horizon, Some(limit))
}

fn simulate_with_limit(
    jobs: &[BudgetJob],
    recommendations: &[Option<u32>],
    horizon: u32,
    limit: Option<NonZeroU32>,
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
        forced_fallback_dispatches: 0,
    };
    let mut recommended_since_fallback = 0_u32;
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
            let force_queue = position.is_some()
                && limit.is_some_and(|max| recommended_since_fallback >= max.get());
            let i = match position.filter(|_| !force_queue) {
                Some(pos) => {
                    if limit.is_some() {
                        recommended_since_fallback += 1;
                    }
                    queue.remove(pos).expect("position belongs to queue")
                }
                None => {
                    report.fallback_dispatches += 1;
                    report.forced_fallback_dispatches += u32::from(force_queue);
                    recommended_since_fallback = 0;
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
        } else {
            recommended_since_fallback = 0;
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

    #[test]
    fn repeated_recommendations_delay_a_job_and_limit_interleaves_it() {
        let jobs = [job(1, 5, 5), job(2, 2, 2)];
        let recommendations = [Some(1); 7];
        let unchecked = simulate_budgeted(&jobs, &recommendations, 7).unwrap();
        let guarded =
            simulate_budgeted_with_limit(&jobs, &recommendations, 7, NonZeroU32::new(2).unwrap())
                .unwrap();
        assert_eq!(
            unchecked.dispatches,
            [(0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (5, 2), (6, 2)]
        );
        assert_eq!(
            guarded.dispatches,
            [(0, 1), (1, 1), (2, 2), (3, 1), (4, 1), (5, 2), (6, 1)]
        );
        assert_eq!(guarded.forced_fallback_dispatches, 2);
        assert!(guarded
            .outcomes
            .contains(&EndState::Completed { id: 1, at: 7 }));
        assert!(guarded
            .outcomes
            .contains(&EndState::Completed { id: 2, at: 6 }));
    }

    #[test]
    fn alternating_recommendations_cannot_evade_the_limit() {
        let jobs = [job(1, 8, 8), job(2, 8, 8), job(3, 8, 8)];
        let recommendations: Vec<_> = (0..24).map(|tick| Some(1 + tick % 2)).collect();
        let report =
            simulate_budgeted_with_limit(&jobs, &recommendations, 24, NonZeroU32::new(2).unwrap())
                .unwrap();
        let first_third = report.dispatches.iter().find(|(_, id)| *id == 3).unwrap().0;
        assert!(
            first_third <= 8,
            "third job waited until tick {first_third}"
        );
        assert_eq!(report.outcomes.len(), 3);
    }

    #[test]
    fn finite_recommendation_patterns_all_give_each_job_a_turn() {
        let jobs = [job(1, 8, 8), job(2, 8, 8), job(3, 8, 8)];
        for pattern in 0_u32..729 {
            let mut value = pattern;
            let mut choices = [0_u32; 6];
            for choice in &mut choices {
                *choice = value % 3 + 1;
                value /= 3;
            }
            let recommendations: Vec<_> = (0..24)
                .map(|tick| Some(choices[tick % choices.len()]))
                .collect();
            let report = simulate_budgeted_with_limit(
                &jobs,
                &recommendations,
                24,
                NonZeroU32::new(2).unwrap(),
            )
            .unwrap();
            for id in 1..=3 {
                let first = report
                    .dispatches
                    .iter()
                    .find(|(_, chosen)| *chosen == id)
                    .unwrap()
                    .0;
                assert!(
                    first <= 8,
                    "pattern {pattern}, job {id}, first tick {first}"
                );
            }
        }
    }

    #[test]
    fn idle_ticks_reset_the_recommendation_count() {
        let mut later = job(2, 1, 1);
        later.job.arrival = 3;
        let report = simulate_budgeted_with_limit(
            &[job(1, 1, 1), later],
            &[Some(1), None, None, Some(2)],
            4,
            NonZeroU32::new(1).unwrap(),
        )
        .unwrap();
        assert_eq!(report.dispatches, [(0, 1), (3, 2)]);
        assert_eq!(report.forced_fallback_dispatches, 0);
    }

    #[test]
    fn invalid_recommendation_resets_the_counter_and_budget_still_applies() {
        let jobs = [job(1, 3, 1), job(2, 3, 3)];
        let report = simulate_budgeted_with_limit(
            &jobs,
            &[Some(1), Some(1), Some(2), Some(2)],
            4,
            NonZeroU32::new(2).unwrap(),
        )
        .unwrap();
        assert_eq!(report.dispatches, [(0, 1), (1, 2), (2, 2), (3, 2)]);
        assert_eq!(report.forced_fallback_dispatches, 0);
        assert!(report.outcomes.contains(&EndState::BudgetExhausted {
            id: 1,
            at: 1,
            remaining: 2
        }));
    }
}
