use crate::{input::Delivery, store::Store};

pub fn apply(store: &mut Store, delivery: Delivery, fault: &str) -> &'static str {
    if !store.cursor.accepts(delivery.seq) {
        return "rejected";
    }
    let Some(total) = store.total.checked_add(delivery.delta) else {
        return "rejected";
    };
    store.total = total;
    if fault == "before" {
        return "failed";
    }
    store.cursor.0 = delivery.seq;
    store.accepted.insert(delivery.seq, delivery);
    if fault == "after" {
        "uncertain"
    } else {
        "applied"
    }
}

pub fn restart(store: &mut Store) {
    store.cursor.0 = 0;
    store.accepted.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_delivery_updates_the_durable_snapshot() {
        let mut store = Store::default();
        assert_eq!(
            apply(
                &mut store,
                Delivery {
                    seq: 1,
                    id: "a".into(),
                    delta: 10
                },
                "none"
            ),
            "applied"
        );
        assert_eq!(
            apply(
                &mut store,
                Delivery {
                    seq: 2,
                    id: "b".into(),
                    delta: -3
                },
                "none"
            ),
            "applied"
        );
        assert_eq!(store.snapshot(), "cursor=2 total=7 accepted=2 pending=0");
    }
}
