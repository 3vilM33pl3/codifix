use crate::interval::Interval;

#[derive(Default)]
pub struct Calendar {
    pub busy: Vec<Interval>,
}

impl Calendar {
    pub fn add(&mut self, interval: Interval) {
        self.busy.push(interval);
    }

    pub fn remove(&mut self, interval: Interval) {
        self.busy
            .retain(|busy| busy.end < interval.start || busy.start > interval.end);
    }

    pub fn free(&self, window: Interval) -> Vec<Interval> {
        if self.busy.contains(&window) {
            Vec::new()
        } else {
            vec![window]
        }
    }
}
