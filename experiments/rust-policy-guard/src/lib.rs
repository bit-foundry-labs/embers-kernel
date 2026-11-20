#![no_std]
#![forbid(unsafe_code)]
//! Research model, NOT an implemented kernel capability table or scheduler.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tenant(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Authority {
    tenant: Tenant,
    max_weight: u16,
    max_pages: u32,
    expiry_epoch: u64,
}
impl Authority {
    /// Only for host tests. In a real OS, the kernel must mint and resolve capabilities.
    pub const fn simulated(
        tenant: Tenant,
        max_weight: u16,
        max_pages: u32,
        expiry_epoch: u64,
    ) -> Self {
        Self {
            tenant,
            max_weight,
            max_pages,
            expiry_epoch,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Proposal {
    pub tenant: Tenant,
    pub epoch: u64,
    pub weight: u16,
    pub pages: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedPolicy {
    tenant: Tenant,
    weight: u16,
    pages: u32,
}
impl ValidatedPolicy {
    pub const fn tenant(self) -> Tenant {
        self.tenant
    }
    pub const fn weight(self) -> u16 {
        self.weight
    }
    pub const fn pages(self) -> u32 {
        self.pages
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Missing,
    WrongTenant,
    Expired,
    ReplayOrStale,
    InvalidWeight,
    MemoryLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Advisory(ValidatedPolicy),
    Baseline(Rejection),
}

/// An asynchronous advisory-policy gate. Invalid suggestions never change state.
pub struct PolicyGate {
    authority: Authority,
    last_epoch: Option<u64>,
}
impl PolicyGate {
    pub const fn new(authority: Authority) -> Self {
        Self {
            authority,
            last_epoch: None,
        }
    }
    pub fn evaluate(&mut self, candidate: Option<Proposal>, now: u64) -> Decision {
        let Some(p) = candidate else {
            return Decision::Baseline(Rejection::Missing);
        };
        let problem = if p.tenant != self.authority.tenant {
            Some(Rejection::WrongTenant)
        } else if now > self.authority.expiry_epoch {
            Some(Rejection::Expired)
        } else if p.epoch != now || self.last_epoch.is_some_and(|e| p.epoch <= e) {
            Some(Rejection::ReplayOrStale)
        } else if p.weight == 0 || p.weight > self.authority.max_weight {
            Some(Rejection::InvalidWeight)
        } else if p.pages > self.authority.max_pages {
            Some(Rejection::MemoryLimit)
        } else {
            None
        };
        if let Some(error) = problem {
            return Decision::Baseline(error);
        }
        self.last_epoch = Some(p.epoch);
        Decision::Advisory(ValidatedPolicy {
            tenant: p.tenant,
            weight: p.weight,
            pages: p.pages,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimumCpu {
    pub tenant: Tenant,
    pub ticks: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    InvalidEpoch,
    InvalidShare,
    Overflow,
    Overbooked,
}

/// Necessary capacity condition in a simplified single-CPU epoch.
/// This does NOT establish fairness or enforce runtime execution.
pub fn admit_minimums(
    epoch: u32,
    reserved_system: u32,
    requests: &[MinimumCpu],
) -> Result<u32, AdmissionError> {
    if epoch == 0 || reserved_system >= epoch {
        return Err(AdmissionError::InvalidEpoch);
    }
    let capacity = epoch - reserved_system;
    let mut total = 0_u32;
    for request in requests {
        if request.ticks == 0 {
            return Err(AdmissionError::InvalidShare);
        }
        total = total
            .checked_add(request.ticks)
            .ok_or(AdmissionError::Overflow)?;
        if total > capacity {
            return Err(AdmissionError::Overbooked);
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: Tenant = Tenant(1);
    const B: Tenant = Tenant(2);
    fn gate() -> PolicyGate {
        PolicyGate::new(Authority::simulated(A, MAX_WEIGHT, MAX_PAGES, EXPIRY))
    }
    fn good(epoch: u64) -> Proposal {
        Proposal {
            tenant: A,
            epoch,
            weight: 100,
            pages: 16,
        }
    }
    #[test]
    fn valid_advice_is_authorized_not_executed() {
        let mut g = gate();
        assert!(matches!(
            g.evaluate(Some(good(1)), 1),
            Decision::Advisory(_)
        ));
    }
    #[test]
    fn model_absence_falls_back() {
        assert_eq!(
            gate().evaluate(None, 1),
            Decision::Baseline(Rejection::Missing)
        );
    }
    #[test]
    fn foreign_tenant_is_denied() {
        let mut g = gate();
        let mut p = good(1);
        p.tenant = B;
        assert_eq!(
            g.evaluate(Some(p), 1),
            Decision::Baseline(Rejection::WrongTenant)
        );
    }
    #[test]
    fn priority_clamping_is_not_substituted_for_rejection() {
        let mut g = gate();
        let mut p = good(1);
        p.weight = 250;
        assert_eq!(
            g.evaluate(Some(p), 1),
            Decision::Baseline(Rejection::InvalidWeight)
        );
        assert!(matches!(
            g.evaluate(Some(good(1)), 1),
            Decision::Advisory(_)
        ));
    }
    #[test]
    fn budgets_are_bounded() {
        let mut g = gate();
        let mut p = good(1);
        p.pages = 65;
        assert_eq!(
            g.evaluate(Some(p), 1),
            Decision::Baseline(Rejection::MemoryLimit)
        );
    }
    #[test]
    fn expired_or_replayed_advice_is_denied() {
        let mut g = gate();
        assert!(matches!(
            g.evaluate(Some(good(2)), 2),
            Decision::Advisory(_)
        ));
        assert_eq!(
            g.evaluate(Some(good(2)), 2),
            Decision::Baseline(Rejection::ReplayOrStale)
        );
        assert_eq!(
            g.evaluate(Some(good(11)), 11),
            Decision::Baseline(Rejection::Expired)
        );
    }
    #[test]
    fn cpu_admission_respects_capacity() {
        let yes = [
            MinimumCpu {
                tenant: A,
                ticks: 40,
            },
            MinimumCpu {
                tenant: B,
                ticks: 40,
            },
        ];
        let no = [
            MinimumCpu {
                tenant: A,
                ticks: 50,
            },
            MinimumCpu {
                tenant: B,
                ticks: 40,
            },
        ];
        assert_eq!(admit_minimums(100, 20, &yes), Ok(80));
        assert_eq!(
            admit_minimums(100, 20, &no),
            Err(AdmissionError::Overbooked)
        );
    }
    #[test]
    fn rejects_invalid_admission_inputs() {
        assert_eq!(admit_minimums(0, 0, &[]), Err(AdmissionError::InvalidEpoch));
        assert_eq!(
            admit_minimums(100, 100, &[]),
            Err(AdmissionError::InvalidEpoch)
        );
        assert_eq!(
            admit_minimums(
                100,
                0,
                &[MinimumCpu {
                    tenant: A,
                    ticks: 0
                }]
            ),
            Err(AdmissionError::InvalidShare)
        );
    }

    // The exhaustive tests below enumerate fixed boundary values instead of
    // random samples, so every run checks the same cases without a
    // dependency. They cover only the listed values and are not a proof.
    const MAX_WEIGHT: u16 = 150;
    const MAX_PAGES: u32 = 64;
    const EXPIRY: u64 = 10;
    const TENANTS: [Tenant; 2] = [A, B];
    // Ascending, so the probe sequence below can advance a gate's replay state.
    const EPOCHS: [u64; 7] = [0, 1, 2, EXPIRY - 1, EXPIRY, EXPIRY + 1, u64::MAX];
    const PRIOR_ACCEPTED: [Option<u64>; 5] =
        [None, Some(0), Some(1), Some(EXPIRY - 1), Some(EXPIRY)];
    const WEIGHTS: [u16; 6] = [0, 1, MAX_WEIGHT - 1, MAX_WEIGHT, MAX_WEIGHT + 1, u16::MAX];
    const PAGES: [u32; 5] = [0, MAX_PAGES - 1, MAX_PAGES, MAX_PAGES + 1, u32::MAX];

    /// Reaches a prior accepted epoch through the public API only.
    fn gate_after(prior: Option<u64>) -> PolicyGate {
        let mut g = gate();
        if let Some(epoch) = prior {
            assert!(matches!(
                g.evaluate(Some(good(epoch)), epoch),
                Decision::Advisory(_)
            ));
        }
        g
    }

    /// Calls `check` for a missing proposal and every boundary proposal.
    fn for_each_candidate(mut check: impl FnMut(Option<Proposal>)) {
        check(None);
        for tenant in TENANTS {
            for epoch in EPOCHS {
                for weight in WEIGHTS {
                    for pages in PAGES {
                        check(Some(Proposal {
                            tenant,
                            epoch,
                            weight,
                            pages,
                        }));
                    }
                }
            }
        }
    }

    /// The policy contract, stated independently of the check order in
    /// `PolicyGate::evaluate`.
    fn within_authority(prior: Option<u64>, candidate: Option<Proposal>, now: u64) -> bool {
        candidate.is_some_and(|p| {
            p.tenant == A
                && now <= EXPIRY
                && p.epoch == now
                && prior.is_none_or(|accepted| p.epoch > accepted)
                && (1..=MAX_WEIGHT).contains(&p.weight)
                && p.pages <= MAX_PAGES
        })
    }

    #[test]
    fn advice_is_accepted_only_within_authority_and_never_widened() {
        for prior in PRIOR_ACCEPTED {
            for now in EPOCHS {
                for_each_candidate(|candidate| {
                    let decision = gate_after(prior).evaluate(candidate, now);
                    let expected = within_authority(prior, candidate, now);
                    match (decision, candidate) {
                        (Decision::Advisory(policy), Some(p)) => {
                            assert!(expected, "accepted {p:?} at {now} after {prior:?}");
                            // Accepted advice must be exactly what was proposed,
                            // never a clamped or substituted value.
                            assert_eq!(
                                (policy.tenant(), policy.weight(), policy.pages()),
                                (p.tenant, p.weight, p.pages)
                            );
                        }
                        (Decision::Advisory(_), None) => panic!("accepted a missing proposal"),
                        (Decision::Baseline(_), _) => {
                            assert!(!expected, "rejected {candidate:?} at {now} after {prior:?}");
                        }
                    }
                });
            }
        }
    }

    #[test]
    fn rejected_advice_leaves_the_gate_unchanged() {
        for prior in PRIOR_ACCEPTED {
            for now in EPOCHS {
                for_each_candidate(|candidate| {
                    let mut gate = gate_after(prior);
                    if matches!(gate.evaluate(candidate, now), Decision::Advisory(_)) {
                        return;
                    }
                    // Compare observable behavior with an untouched gate in the
                    // same prior state. If the rejection consumed a replay epoch,
                    // the next valid proposal would diverge here.
                    let mut untouched = gate_after(prior);
                    for probe in EPOCHS {
                        assert_eq!(
                            gate.evaluate(Some(good(probe)), probe),
                            untouched.evaluate(Some(good(probe)), probe),
                            "rejecting {candidate:?} at {now} after {prior:?} changed the gate"
                        );
                    }
                });
            }
        }
    }

    fn cpu(tenant: u32, ticks: u32) -> MinimumCpu {
        MinimumCpu {
            tenant: Tenant(tenant),
            ticks,
        }
    }

    #[test]
    fn cpu_minimums_are_admitted_only_when_every_share_fits() {
        const SMALL: [u32; 5] = [0, 1, 2, 3, 10];
        for epoch in SMALL {
            for reserved in SMALL {
                for len in 0..=3_usize {
                    for code in 0..SMALL.len().pow(len as u32) {
                        let mut requests = [cpu(0, 0); 3];
                        let mut digits = code;
                        for (index, request) in requests.iter_mut().take(len).enumerate() {
                            *request = cpu(index as u32 + 1, SMALL[digits % SMALL.len()]);
                            digits /= SMALL.len();
                        }
                        let requests = &requests[..len];
                        let result = admit_minimums(epoch, reserved, requests);
                        if epoch == 0 || reserved >= epoch {
                            assert_eq!(result, Err(AdmissionError::InvalidEpoch));
                            continue;
                        }
                        // Sum in u64 so the expectation itself cannot overflow.
                        let total: u64 = requests.iter().map(|r| u64::from(r.ticks)).sum();
                        let fits = requests.iter().all(|r| r.ticks > 0)
                            && total <= u64::from(epoch - reserved);
                        if fits {
                            assert_eq!(result, Ok(total as u32), "{requests:?}");
                        } else {
                            assert!(
                                result.is_err(),
                                "admitted {requests:?} in {epoch}/{reserved}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn exact_capacity_is_admitted_and_one_more_tick_is_overbooked() {
        let exact = [cpu(1, 30), cpu(2, 30), cpu(3, 30)];
        let over = [cpu(1, 30), cpu(2, 30), cpu(3, 31)];
        assert_eq!(admit_minimums(100, 10, &exact), Ok(90));
        assert_eq!(
            admit_minimums(100, 10, &over),
            Err(AdmissionError::Overbooked)
        );
    }

    #[test]
    fn tick_sum_overflow_is_rejected_instead_of_wrapping() {
        // A wrapping sum of these two requests is 0, which would fit any
        // capacity, so admission must detect the overflow explicitly.
        assert_eq!(
            admit_minimums(u32::MAX, 0, &[cpu(1, u32::MAX), cpu(2, 1)]),
            Err(AdmissionError::Overflow)
        );
        assert_eq!(
            admit_minimums(u32::MAX, 0, &[cpu(1, u32::MAX - 1), cpu(2, 1)]),
            Ok(u32::MAX)
        );
        assert_eq!(
            admit_minimums(u32::MAX, 1, &[cpu(1, u32::MAX)]),
            Err(AdmissionError::Overbooked)
        );
    }
}
