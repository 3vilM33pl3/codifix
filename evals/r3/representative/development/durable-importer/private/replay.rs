use crate::{input::Delivery, store::Store};

pub fn apply(store: &mut Store, delivery: Delivery, fault: &str) -> &'static str {
    // Identities and authority positions are immutable, including queued writes.
    for previous in store.accepted.values().chain(store.pending.values()) {
        if previous.id == delivery.id || previous.seq == delivery.seq {
            return if previous == &delivery {
                "duplicate"
            } else {
                "rejected"
            };
        }
    }
    if delivery.seq <= store.cursor.0 {
        return "rejected";
    }
    let mut candidate = store.clone();
    candidate.pending.insert(delivery.seq, delivery);
    let mut applied = false;
    while let Some(next) = candidate.cursor.0.checked_add(1) {
        let Some(delivery) = candidate.pending.remove(&next) else {
            break;
        };
        if !candidate.cursor.accepts(delivery.seq) {
            return "rejected";
        }
        let Some(total) = candidate.total.checked_add(delivery.delta) else {
            return "rejected";
        };
        candidate.total = total;
        candidate.cursor.0 = delivery.seq;
        candidate.accepted.insert(delivery.seq, delivery);
        applied = true;
    }
    if fault == "before" {
        return "failed";
    }
    *store = candidate;
    if fault == "after" {
        "uncertain"
    } else if applied {
        "applied"
    } else {
        "queued"
    }
}

pub fn restart(_: &mut Store) {
    // The caller retains the durable Store while replacing the worker.
}
