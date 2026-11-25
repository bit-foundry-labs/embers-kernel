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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservedState {
    NotArrived,
    Runnable,
    Completed,
    BudgetExhausted,
}

/// Waits count whole simulated ticks when a job is runnable but not running.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedJob {
    pub id: u32,
    pub state: ObservedState,
    /// None means the job has never been dispatched.
    pub first_wait: Option<u32>,
    /// None means the job has not been runnable during the observation.
    pub longest_wait: Option<u32>,
    /// A trailing wait is reported only for a job still runnable at the end.
    pub waiting_at_end: Option<u32>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Observation {
    pub report: Report,
    /// Exclusive end of the observed ticks; may be less than the horizon.
    pub observed_until: u32,
    pub complete: bool,
    pub jobs: Vec<ObservedJob>,
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
    let observation = observe_budgeted(jobs, recommendations, horizon, limit)?;
    if observation.complete {
        Ok(observation.report)
    } else {
        Err(BudgetError::HorizonExceeded)
    }
}

/// Observe complete or partial finite runs without changing completed-run APIs.
/// `None` accepts all valid recommendations; `Some` applies the queue limit.
pub fn observe_budgeted(
    jobs: &[BudgetJob],
    recommendations: &[Option<u32>],
    horizon: u32,
    limit: Option<NonZeroU32>,
) -> Result<Observation, BudgetError> {
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
    let mut states = vec![ObservedState::NotArrived; jobs.len()];
    let mut first_wait = vec![None; jobs.len()];
    let mut longest_wait = vec![None; jobs.len()];
    let mut last_run = vec![None; jobs.len()];
    let mut report = Report {
        dispatches: Vec::new(),
        outcomes: Vec::new(),
        fallback_dispatches: 0,
        forced_fallback_dispatches: 0,
    };
    let mut recommended_since_fallback = 0_u32;
    let mut observed_until = 0;
    for tick in 0..horizon {
        if jobs.is_empty() {
            break;
        }
        for (i, item) in jobs.iter().enumerate() {
            if item.job.arrival == tick {
                if budget[i] == 0 {
                    states[i] = ObservedState::BudgetExhausted;
                    report.outcomes.push(EndState::BudgetExhausted {
                        id: item.job.id,
                        at: tick,
                        remaining: remaining[i],
                    });
                } else {
                    states[i] = ObservedState::Runnable;
                    longest_wait[i] = Some(0_u32);
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
            let wait_start = last_run[i].map_or(jobs[i].job.arrival, |last| last + 1);
            let waited = tick - wait_start;
            if first_wait[i].is_none() {
                first_wait[i] = Some(waited);
            }
            longest_wait[i] = Some(longest_wait[i].unwrap_or(0).max(waited));
            last_run[i] = Some(tick);
            report.dispatches.push((tick, jobs[i].job.id));
            remaining[i] -= 1;
            budget[i] -= 1;
            if remaining[i] == 0 {
                states[i] = ObservedState::Completed;
                report.outcomes.push(EndState::Completed {
                    id: jobs[i].job.id,
                    at: tick + 1,
                });
            } else if budget[i] == 0 {
                states[i] = ObservedState::BudgetExhausted;
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
        observed_until = tick + 1;
        if report.outcomes.len() == jobs.len() {
            break;
        }
    }
    let observations = jobs
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let waiting_at_end = if states[i] == ObservedState::Runnable {
                let wait_start = last_run[i].map_or(item.job.arrival, |last| last + 1);
                let trailing = observed_until - wait_start;
                longest_wait[i] = Some(longest_wait[i].unwrap_or(0).max(trailing));
                Some(trailing)
            } else {
                None
            };
            ObservedJob {
                id: item.job.id,
                state: states[i],
                first_wait: first_wait[i],
                longest_wait: longest_wait[i],
                waiting_at_end,
            }
        })
        .collect();
    Ok(Observation {
        complete: report.outcomes.len() == jobs.len(),
        report,
        observed_until,
        jobs: observations,
    })
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
    fn partial_trace_shows_a_job_that_has_never_run() {
        let jobs = [job(1, 5, 5), job(2, 2, 2)];
        let suggestions = [Some(1); 4];
        let observed = observe_budgeted(&jobs, &suggestions, 4, None).unwrap();
        assert!(!observed.complete);
        assert_eq!(observed.observed_until, 4);
        assert_eq!(observed.report.dispatches, [(0, 1), (1, 1), (2, 1), (3, 1)]);
        assert_eq!(
            observed.jobs[1],
            ObservedJob {
                id: 2,
                state: ObservedState::Runnable,
                first_wait: None,
                longest_wait: Some(4),
                waiting_at_end: Some(4)
            }
        );
        assert_eq!(
            simulate_budgeted(&jobs, &suggestions, 4),
            Err(BudgetError::HorizonExceeded)
        );
    }

    #[test]
    fn counts_gaps_and_trailing_wait_for_the_same_traces() {
        let jobs = [job(1, 5, 5), job(2, 2, 2)];
        let suggestions = [Some(1); 4];
        let limited = observe_budgeted(&jobs, &suggestions, 4, NonZeroU32::new(2)).unwrap();
        let round_robin = observe_budgeted(&jobs, &[], 4, None).unwrap();
        assert_eq!(limited.report.dispatches, [(0, 1), (1, 1), (2, 2), (3, 1)]);
        assert_eq!(
            limited.jobs[1],
            ObservedJob {
                id: 2,
                state: ObservedState::Runnable,
                first_wait: Some(2),
                longest_wait: Some(2),
                waiting_at_end: Some(1)
            }
        );
        assert_eq!(
            round_robin.report.dispatches,
            [(0, 1), (1, 2), (2, 1), (3, 2)]
        );
        assert_eq!(
            round_robin.jobs[1],
            ObservedJob {
                id: 2,
                state: ObservedState::Completed,
                first_wait: Some(1),
                longest_wait: Some(1),
                waiting_at_end: None
            }
        );
        assert_eq!(round_robin.jobs[0].waiting_at_end, Some(1));
    }

    #[test]
    fn ignores_future_and_exhausted_jobs_when_measuring_runnable_wait() {
        let mut later = job(3, 1, 1);
        later.job.arrival = 3;
        let jobs = [job(1, 1, 0), job(2, 3, 1), later];
        let observed = observe_budgeted(&jobs, &[], 2, None).unwrap();
        assert_eq!(
            observed.jobs[0],
            ObservedJob {
                id: 1,
                state: ObservedState::BudgetExhausted,
                first_wait: None,
                longest_wait: None,
                waiting_at_end: None
            }
        );
        assert_eq!(
            observed.jobs[1],
            ObservedJob {
                id: 2,
                state: ObservedState::BudgetExhausted,
                first_wait: Some(0),
                longest_wait: Some(0),
                waiting_at_end: None
            }
        );
        assert_eq!(
            observed.jobs[2],
            ObservedJob {
                id: 3,
                state: ObservedState::NotArrived,
                first_wait: None,
                longest_wait: None,
                waiting_at_end: None
            }
        );
        let zero = observe_budgeted(&jobs, &[], 0, None).unwrap();
        assert_eq!(zero.observed_until, 0);
        assert!(zero
            .jobs
            .iter()
            .all(|item| item.state == ObservedState::NotArrived));
    }

    #[test]
    fn completed_observations_match_existing_apis() {
        let jobs = [job(1, 5, 5), job(2, 2, 2)];
        let recommendations = [Some(1); 7];
        let unlimited = observe_budgeted(&jobs, &recommendations, 12, None).unwrap();
        assert!(unlimited.complete);
        assert_eq!(unlimited.observed_until, 7);
        assert_eq!(
            unlimited.report,
            simulate_budgeted(&jobs, &recommendations, 12).unwrap()
        );
        let limit = NonZeroU32::new(2).unwrap();
        let limited = observe_budgeted(&jobs, &recommendations, 12, Some(limit)).unwrap();
        assert!(limited.complete);
        assert_eq!(
            limited.report,
            simulate_budgeted_with_limit(&jobs, &recommendations, 12, limit).unwrap()
        );
        assert!(observe_budgeted(&[], &[], 0, None).unwrap().complete);
        assert_eq!(simulate_budgeted(&[], &[], 0).unwrap().dispatches.len(), 0);
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
    fn staggered_arrivals_and_adversarial_hints_stay_within_candidate_wait() {
        // Three jobs stay runnable with remaining work and budget throughout
        // the nine-tick observation. This is a finite search, not a proof.
        let patterns = 5_usize.pow(6);
        for arrivals in [[0, 0, 0], [0, 1, 2], [2, 0, 1]] {
            let mut jobs = [job(1, 20, 20), job(2, 20, 20), job(3, 20, 20)];
            for (item, arrival) in jobs.iter_mut().zip(arrivals) {
                item.job.arrival = arrival;
            }
            for limit in [1_u32, 2] {
                let candidate = jobs.len() as u32 * (limit + 1) - 1;
                for pattern in 0..patterns {
                    let mut value = pattern;
                    let mut choices = [None; 6];
                    for choice in &mut choices {
                        *choice = match value % 5 {
                            0 => None,
                            1 => Some(1),
                            2 => Some(2),
                            3 => Some(3),
                            _ => Some(999),
                        };
                        value /= 5;
                    }
                    let recommendations: Vec<_> =
                        (0..9).map(|tick| choices[tick % choices.len()]).collect();
                    let observation =
                        observe_budgeted(&jobs, &recommendations, 9, NonZeroU32::new(limit))
                            .unwrap();
                    for item in observation.jobs {
                        assert!(
                            item.longest_wait.unwrap() <= candidate,
                            "arrivals={arrivals:?} limit={limit} pattern={pattern} job={item:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unlimited_suggestions_exceed_the_candidate_limit_example() {
        let jobs = [job(1, 20, 20), job(2, 20, 20), job(3, 20, 20)];
        let observation = observe_budgeted(&jobs, &[Some(1); 9], 9, None).unwrap();
        assert_eq!(observation.jobs[1].first_wait, None);
        assert_eq!(observation.jobs[1].longest_wait, Some(9));
        // For three jobs and a recommendation limit of two, the candidate
        // finite-model threshold is 3 * (2 + 1) - 1 = 8 ticks.
        assert!(observation.jobs[1].longest_wait.unwrap() > 8);
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
