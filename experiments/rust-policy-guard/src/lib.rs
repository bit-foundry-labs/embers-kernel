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
        PolicyGate::new(Authority::simulated(A, 150, 64, 10))
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
}
