#!/usr/bin/env python3
"""Bounded IPC schedule explorer; no kernel or hardware proof."""
from collections import deque
from dataclasses import dataclass, replace

ACTORS = range(2)
FULL = 3
Q = 2


@dataclass(frozen=True)
class State:
    send_rights: int = FULL
    recv_rights: int = FULL
    staged: int = 0
    sent: int = 0
    queue: tuple = ()
    reserved: tuple = (-1, -1)
    done: int = 0
    closed: bool = False
    retired: bool = False


def set_at(pair, index, value):
    return tuple(value if j == index else x for j, x in enumerate(pair))


def moves(s, mutant):
    if s.retired:
        return
    for i in ACTORS:
        bit = 1 << i
        if s.send_rights & bit:
            yield ('revoke_send', i), replace(s, send_rights=s.send_rights ^ bit)
        if s.recv_rights & bit:
            yield ('revoke_recv', i), replace(s, recv_rights=s.recv_rights ^ bit)
        if not s.closed and s.send_rights & bit and not (s.staged | s.sent) & bit:
            yield ('stage', i), replace(s, staged=s.staged | bit)
        if s.staged & bit:
            yield ('abort_send', i), replace(s, staged=s.staged ^ bit)
            if not s.closed and len(s.queue) < Q and (s.send_rights & bit or mutant == 'send'):
                yield ('commit', i), replace(s, staged=s.staged ^ bit,
                                             sent=s.sent | bit, queue=s.queue + (i,))
        if not s.closed and s.queue and not s.done & bit and s.reserved[i] == -1:
            if (s.recv_rights & bit or mutant == 'receive') and (
                s.reserved == (-1, -1) or mutant == 'exclusive'
            ):
                yield ('admit', i), replace(s, reserved=set_at(s.reserved, i, s.queue[0]))
        if s.reserved[i] != -1:
            released = set_at(s.reserved, i, -1)
            yield ('copy_failed', i), replace(s, reserved=released, done=s.done | bit)
            if s.queue and s.queue[0] == s.reserved[i]:
                yield ('copy_ok', i), replace(s, reserved=released,
                                              queue=s.queue[1:], done=s.done | bit)
    if not s.closed:
        yield ('close', -1), replace(s, closed=True)
    if s.closed and (not s.staged and s.reserved == (-1, -1) or mutant == 'retire'):
        yield ('retire', -1), replace(s, queue=(), retired=True)


def violation(s, event, t):
    """Check observed transitions independently of the mutation selector."""
    action, i = event
    if action == 'commit' and (s.closed or not (s.send_rights >> i) & 1):
        return 'late_send'
    if action == 'admit' and (s.closed or not (s.recv_rights >> i) & 1):
        return 'late_receive'
    if sum(x != -1 for x in t.reserved) > 1:
        return 'exclusive_head'
    if action == 'retire' and (s.staged or any(x != -1 for x in s.reserved)):
        return 'early_retire'
    if len(t.queue) > Q or len(set(t.queue)) != len(t.queue):
        return 'queue_bounds'
    if any(not (t.sent >> x) & 1 for x in t.queue) or t.sent & t.staged:
        return 'accounting'
    if action == 'commit':
        expected = s.queue + (i,)
    elif action == 'copy_ok':
        if not s.queue or s.reserved[i] != s.queue[0]:
            return 'wrong_head'
        expected = s.queue[1:]
    elif action == 'retire':
        expected = ()
    else:
        expected = s.queue
    if t.queue != expected:
        return 'fifo_or_failure_mutation'
    return None


def explore(mutant=None):
    initial = State()
    seen = {initial}
    pending = deque([(initial, ())])
    edges = 0
    while pending:
        state, trace = pending.popleft()
        for event, target in moves(state, mutant):
            edges += 1
            failure = violation(state, event, target)
            if failure:
                return len(seen), edges, failure, trace + (event,)
            if target not in seen:
                seen.add(target)
                pending.append((target, trace + (event,)))
    return len(seen), edges, None, ()


def main():
    cases = ((None, None), ('send', 'late_send'), ('receive', 'late_receive'),
             ('exclusive', 'exclusive_head'), ('retire', 'early_retire'))
    for mutant, expected in cases:
        states, edges, failure, trace = explore(mutant)
        if failure != expected:
            raise SystemExit(f'{mutant}: expected {expected}, got {failure}')
        print(f'{mutant or "safe"}: {states} states, {edges} edges; {failure or "PASS"}')
        if trace:
            print('  ' + ' -> '.join(f'{a}({i})' for a, i in trace))


if __name__ == '__main__':
    main()
