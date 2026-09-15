use crate::interval::Interval;

#[derive(Default)]
pub struct Calendar {
    pub busy: Vec<Interval>,
}

impl Calendar {
    pub fn add(&mut self, interval: Interval) {
        self.busy.push(interval);
        self.busy.sort_by_key(|span| (span.start, span.end));
        let mut merged: Vec<Interval> = Vec::new();
        for span in self.busy.drain(..) {
            if let Some(last) = merged.last_mut() {
                if span.start <= last.end {
                    last.end = last.end.max(span.end);
                    continue;
                }
            }
            merged.push(span);
        }
        self.busy = merged;
    }

    pub fn remove(&mut self, removed: Interval) {
        let mut next = Vec::new();
        for span in &self.busy {
            if span.end <= removed.start || span.start >= removed.end {
                next.push(*span);
                continue;
            }
            if span.start < removed.start {
                next.push(Interval {
                    start: span.start,
                    end: removed.start,
                });
            }
            if span.end > removed.end {
                next.push(Interval {
                    start: removed.end,
                    end: span.end,
                });
            }
        }
        self.busy = next;
    }

    pub fn free(&self, window: Interval) -> Vec<Interval> {
        let mut result = Vec::new();
        let mut cursor = window.start;
        for span in &self.busy {
            if span.end <= cursor || span.start >= window.end {
                continue;
            }
            if span.start > cursor {
                result.push(Interval {
                    start: cursor,
                    end: span.start.min(window.end),
                });
            }
            cursor = cursor.max(span.end.min(window.end));
        }
        if cursor < window.end {
            result.push(Interval {
                start: cursor,
                end: window.end,
            });
        }
        result
    }
}
