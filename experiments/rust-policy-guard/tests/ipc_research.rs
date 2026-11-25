#![no_std]
#![forbid(unsafe_code)]

//! Host-only serialized IPC reference model. Actor identities, rights changes,
//! and copy completion are simulated inputs, not trusted kernel mechanisms.

const CAPACITY: usize = 2;
const MAX_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Actor {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Handle {
    actor: Actor,
    generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ticket {
    handle: Handle,
    serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SendTicket(Ticket);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReceiveTicket(Ticket);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Message {
    bytes: [u8; MAX_BYTES],
    len: usize,
}

impl Message {
    fn new(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        let mut message = Self {
            bytes: [0; MAX_BYTES],
            len: bytes.len(),
        };
        message.bytes[..bytes.len()].copy_from_slice(bytes);
        Ok(message)
    }

    fn payload(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    Unauthorized,
    Stale,
    Closed,
    Busy,
    Full,
    Empty,
    TooLarge,
    TooSmall,
    CopyFailed,
    Exhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Open,
    Closing,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Staged {
    ticket: SendTicket,
    message: Message,
}

struct Endpoint {
    generation: u32,
    serial: u32,
    phase: Phase,
    send_rights: [bool; 2],
    receive_rights: [bool; 2],
    staged: [Option<Staged>; 2],
    queue: [Option<Message>; CAPACITY],
    len: usize,
    reservation: Option<ReceiveTicket>,
}

impl Endpoint {
    fn new() -> Self {
        Self {
            generation: 1,
            serial: 0,
            phase: Phase::Open,
            send_rights: [true; 2],
            receive_rights: [true; 2],
            staged: [None; 2],
            queue: [None; CAPACITY],
            len: 0,
            reservation: None,
        }
    }

    // Simulated trusted mint; not a public capability construction interface.
    fn handle(&self, actor: Actor) -> Handle {
        Handle {
            actor,
            generation: self.generation,
        }
    }

    fn identity(&self, caller: Actor, handle: Handle) -> Result<(), Error> {
        if caller != handle.actor {
            return Err(Error::Unauthorized);
        }
        if handle.generation != self.generation {
            return Err(Error::Stale);
        }
        Ok(())
    }

    fn authorize(&self, caller: Actor, handle: Handle, send: bool) -> Result<(), Error> {
        self.identity(caller, handle)?;
        if self.phase != Phase::Open {
            return Err(Error::Closed);
        }
        let rights = if send {
            self.send_rights
        } else {
            self.receive_rights
        };
        if !rights[caller as usize] {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    fn ticket(&mut self, handle: Handle) -> Result<Ticket, Error> {
        self.serial = self.serial.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(Ticket {
            handle,
            serial: self.serial,
        })
    }

    fn stage(&mut self, caller: Actor, handle: Handle, bytes: &[u8]) -> Result<SendTicket, Error> {
        self.authorize(caller, handle, true)?;
        if self.staged[caller as usize].is_some() {
            return Err(Error::Busy);
        }
        let message = Message::new(bytes)?;
        let ticket = SendTicket(self.ticket(handle)?);
        self.staged[caller as usize] = Some(Staged { ticket, message });
        Ok(ticket)
    }

    fn commit(&mut self, caller: Actor, ticket: SendTicket) -> Result<(), Error> {
        self.identity(caller, ticket.0.handle)?;
        let slot = caller as usize;
        let staged = self.staged[slot].ok_or(Error::Stale)?;
        if staged.ticket != ticket {
            return Err(Error::Stale);
        }
        // Consume only this exact hold, even on revoked/full/closed rejection.
        self.staged[slot] = None;
        self.authorize(caller, ticket.0.handle, true)?;
        if self.len == CAPACITY {
            return Err(Error::Full);
        }
        self.queue[self.len] = Some(staged.message);
        self.len += 1;
        Ok(())
    }

    fn admit_receive(
        &mut self,
        caller: Actor,
        handle: Handle,
        capacity: usize,
    ) -> Result<ReceiveTicket, Error> {
        self.authorize(caller, handle, false)?;
        if self.reservation.is_some() {
            return Err(Error::Busy);
        }
        let message = self.queue[0].ok_or(Error::Empty)?;
        if capacity < message.len {
            return Err(Error::TooSmall);
        }
        let ticket = ReceiveTicket(self.ticket(handle)?);
        self.reservation = Some(ticket);
        Ok(ticket)
    }

    // A tracked pre-revoke receive can finish. Real copying, mapping lifetimes
    // and cancellation are outside this model; no caller controls real holds.
    fn finish_receive(
        &mut self,
        caller: Actor,
        ticket: ReceiveTicket,
        copy_succeeded: bool,
    ) -> Result<Message, Error> {
        self.identity(caller, ticket.0.handle)?;
        if self.reservation != Some(ticket) {
            return Err(Error::Stale);
        }
        self.reservation = None;
        if !copy_succeeded {
            return Err(Error::CopyFailed);
        }
        let message = self.queue[0].ok_or(Error::Empty)?;
        self.queue[0] = self.queue[1].take();
        self.len -= 1;
        Ok(message)
    }

    fn revoke_send(&mut self, actor: Actor) {
        self.send_rights[actor as usize] = false;
    }

    fn revoke_receive(&mut self, actor: Actor) {
        self.receive_rights[actor as usize] = false;
    }

    fn close(&mut self) {
        if self.phase == Phase::Open {
            self.phase = Phase::Closing;
        }
    }

    fn retire(&mut self) -> Result<(), Error> {
        if self.phase != Phase::Closing {
            return Err(Error::Closed);
        }
        if self.staged.iter().any(Option::is_some) || self.reservation.is_some() {
            return Err(Error::Busy);
        }
        self.queue = [None; CAPACITY];
        self.len = 0;
        self.phase = Phase::Retired;
        Ok(())
    }

    fn reuse(&mut self) -> Result<(), Error> {
        if self.phase != Phase::Retired {
            return Err(Error::Busy);
        }
        let generation = self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        *self = Self::new();
        self.generation = generation;
        Ok(())
    }
}

fn send(endpoint: &mut Endpoint, actor: Actor, payload: &[u8]) -> Result<(), Error> {
    let ticket = endpoint.stage(actor, endpoint.handle(actor), payload)?;
    endpoint.commit(actor, ticket)
}

fn receive(endpoint: &mut Endpoint, actor: Actor) -> Result<Message, Error> {
    let ticket = endpoint.admit_receive(actor, endpoint.handle(actor), MAX_BYTES)?;
    endpoint.finish_receive(actor, ticket, true)
}

#[test]
fn staged_copy_is_owned_and_fifo_uses_commit_order() {
    let mut e = Endpoint::new();
    let mut source = [1, 2];
    let a = e.stage(Actor::A, e.handle(Actor::A), &source).unwrap();
    source[0] = 9;
    assert_eq!(source[0], 9);
    send(&mut e, Actor::B, &[3]).unwrap();
    e.commit(Actor::A, a).unwrap();
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[3]);
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[1, 2]);
    assert_eq!(receive(&mut e, Actor::B), Err(Error::Empty));
}

#[test]
fn revoked_commit_releases_staging_without_enqueuing() {
    let mut e = Endpoint::new();
    let ticket = e.stage(Actor::A, e.handle(Actor::A), &[1]).unwrap();
    e.revoke_send(Actor::A);
    assert_eq!(e.commit(Actor::A, ticket), Err(Error::Unauthorized));
    assert_eq!(e.staged, [None; 2]);
    assert_eq!(e.len, 0);
    e.close();
    assert_eq!(e.retire(), Ok(()));
}

#[test]
fn full_queue_rejects_and_releases_exact_hold() {
    let mut e = Endpoint::new();
    send(&mut e, Actor::A, &[1]).unwrap();
    send(&mut e, Actor::B, &[2]).unwrap();
    let ticket = e.stage(Actor::A, e.handle(Actor::A), &[3]).unwrap();
    assert_eq!(e.commit(Actor::A, ticket), Err(Error::Full));
    assert_eq!(e.staged, [None; 2]);
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[1]);
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[2]);
}

#[test]
fn receive_reservation_survives_revocation_but_new_admission_fails() {
    let mut e = Endpoint::new();
    send(&mut e, Actor::A, &[7]).unwrap();
    let ticket = e.admit_receive(Actor::B, e.handle(Actor::B), 4).unwrap();
    e.revoke_receive(Actor::B);
    assert_eq!(
        e.admit_receive(Actor::A, e.handle(Actor::A), 4),
        Err(Error::Busy)
    );
    assert_eq!(
        e.finish_receive(Actor::B, ticket, true).unwrap().payload(),
        &[7]
    );
    assert_eq!(
        e.admit_receive(Actor::B, e.handle(Actor::B), 4),
        Err(Error::Unauthorized)
    );
}

#[test]
fn failed_copy_retains_head_and_stale_ticket_cannot_release_new_hold() {
    let mut e = Endpoint::new();
    send(&mut e, Actor::A, &[5]).unwrap();
    let old = e.admit_receive(Actor::B, e.handle(Actor::B), 4).unwrap();
    assert_eq!(
        e.finish_receive(Actor::B, old, false),
        Err(Error::CopyFailed)
    );
    let new = e.admit_receive(Actor::B, e.handle(Actor::B), 4).unwrap();
    assert_eq!(e.finish_receive(Actor::B, old, true), Err(Error::Stale));
    assert_eq!(
        e.finish_receive(Actor::A, new, true),
        Err(Error::Unauthorized)
    );
    assert_eq!(
        e.finish_receive(Actor::B, new, true).unwrap().payload(),
        &[5]
    );
}

#[test]
fn length_limits_and_zero_length_messages() {
    let mut e = Endpoint::new();
    assert_eq!(
        send(&mut e, Actor::A, &[0; MAX_BYTES + 1]),
        Err(Error::TooLarge)
    );
    send(&mut e, Actor::A, &[1, 2]).unwrap();
    assert_eq!(
        e.admit_receive(Actor::B, e.handle(Actor::B), 1),
        Err(Error::TooSmall)
    );
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[1, 2]);
    send(&mut e, Actor::A, &[]).unwrap();
    let ticket = e.admit_receive(Actor::B, e.handle(Actor::B), 0).unwrap();
    assert!(e
        .finish_receive(Actor::B, ticket, true)
        .unwrap()
        .payload()
        .is_empty());
}

#[test]
fn closure_drains_holds_before_reuse_and_rejects_stale_handles() {
    let mut e = Endpoint::new();
    let old = e.handle(Actor::A);
    send(&mut e, Actor::A, &[1]).unwrap();
    let recv = e.admit_receive(Actor::B, e.handle(Actor::B), 4).unwrap();
    let pending = e.stage(Actor::A, old, &[2]).unwrap();
    e.close();
    assert_eq!(e.retire(), Err(Error::Busy));
    assert_eq!(e.commit(Actor::A, pending), Err(Error::Closed));
    assert_eq!(e.retire(), Err(Error::Busy));
    e.finish_receive(Actor::B, recv, true).unwrap();
    e.retire().unwrap();
    e.reuse().unwrap();
    assert_eq!(e.stage(Actor::A, old, &[3]), Err(Error::Stale));
    assert_eq!(e.commit(Actor::A, pending), Err(Error::Stale));
    assert_eq!(e.finish_receive(Actor::B, recv, true), Err(Error::Stale));
    assert_eq!(receive(&mut e, Actor::B), Err(Error::Empty));
}

#[test]
fn exhausted_generations_and_tickets_never_wrap() {
    let mut e = Endpoint::new();
    e.serial = u32::MAX;
    assert_eq!(
        e.stage(Actor::A, e.handle(Actor::A), &[1]),
        Err(Error::Exhausted)
    );
    assert_eq!(e.staged, [None; 2]);
    e.generation = u32::MAX;
    e.close();
    e.retire().unwrap();
    assert_eq!(e.reuse(), Err(Error::Exhausted));
    assert_eq!(e.phase, Phase::Retired);
}

#[test]
fn old_send_ticket_cannot_consume_new_staging() {
    let mut e = Endpoint::new();
    let old = e.stage(Actor::A, e.handle(Actor::A), &[1]).unwrap();
    e.commit(Actor::A, old).unwrap();
    let new = e.stage(Actor::A, e.handle(Actor::A), &[2]).unwrap();
    assert_eq!(e.commit(Actor::A, old), Err(Error::Stale));
    assert_eq!(e.commit(Actor::B, new), Err(Error::Unauthorized));
    e.commit(Actor::A, new).unwrap();
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[1]);
    assert_eq!(receive(&mut e, Actor::B).unwrap().payload(), &[2]);
}
