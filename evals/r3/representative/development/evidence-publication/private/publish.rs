use crate::{
    capture::Capture,
    policy::Policy,
    review::{self, Review, Snapshot},
};
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Published {
    pub request: String,
    pub review: String,
    pub publisher: String,
    pub snapshot: Snapshot,
}
pub struct Proposal {
    pub base: usize,
    pub editor: String,
    pub body: String,
}
#[derive(Default)]
pub struct State {
    pub capture: Option<Capture>,
    pub policy: Policy,
    pub log: Vec<Published>,
    reviews: BTreeMap<String, Review>,
    proposals: BTreeMap<String, Proposal>,
}
impl State {
    pub fn review(&mut self, id: &str, publisher: &str) -> &'static str {
        if self.reviews.contains_key(id) || !self.policy.allows(publisher) {
            return "rejected";
        }
        let Some(capture) = &self.capture else {
            return "rejected";
        };
        self.reviews.insert(
            id.into(),
            review::prepare(capture, publisher, self.log.len() + 1),
        );
        "reviewed"
    }
    pub fn propose(&mut self, id: &str, base: usize, editor: &str, body: &str) -> &'static str {
        if self.proposals.contains_key(id) || base == 0 || base != self.log.len() {
            return "rejected";
        }
        self.proposals.insert(
            id.into(),
            Proposal {
                base,
                editor: editor.into(),
                body: body.into(),
            },
        );
        "proposed"
    }
    pub fn approve(&mut self, proposal: &str, id: &str, publisher: &str) -> &'static str {
        if self.reviews.contains_key(id) || !self.policy.allows(publisher) {
            return "rejected";
        }
        let Some(p) = self.proposals.get(proposal) else {
            return "rejected";
        };
        if p.base != self.log.len() {
            return "rejected";
        }
        let Some(last) = self.log.last() else {
            return "rejected";
        };
        let snapshot = Snapshot {
            version: p.base + 1,
            author: last.snapshot.author.clone(),
            body: p.body.clone(),
            editor: p.editor.clone(),
        };
        self.reviews.insert(
            id.into(),
            Review {
                publisher: publisher.into(),
                snapshot,
            },
        );
        "reviewed"
    }
    pub fn send(
        &mut self,
        request: &str,
        review: &str,
        publisher: &str,
        fault: &str,
    ) -> &'static str {
        if let Some(previous) = self.log.iter().find(|e| e.request == request) {
            return if previous.review == review && previous.publisher == publisher {
                "duplicate"
            } else {
                "rejected"
            };
        }
        if !self.policy.allows(publisher) {
            return "rejected";
        }
        let Some(reviewed) = self.reviews.get(review) else {
            return "rejected";
        };
        if reviewed.publisher != publisher || reviewed.snapshot.version != self.log.len() + 1 {
            return "rejected";
        }
        if fault == "before" {
            return "failed";
        }
        let snapshot = reviewed.snapshot.clone();
        self.log.push(Published {
            request: request.into(),
            review: review.into(),
            publisher: publisher.into(),
            snapshot,
        });
        if fault == "after" {
            "uncertain"
        } else {
            "published"
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_reviewed_publication() {
        let mut s = State::default();
        s.policy.grants.insert("bob".into());
        s.capture = Some(Capture {
            author: "alice".into(),
            body: "old".into(),
        });
        assert_eq!(s.review("r1", "bob"), "reviewed");
        assert_eq!(s.send("q1", "r1", "bob", "none"), "published");
        assert_eq!(s.log[0].snapshot.body, "old");
    }
}
