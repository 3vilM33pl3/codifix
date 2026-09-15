#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    delivered: usize,
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Open,
    Closed,
    Poisoned,
}

impl Decoder {
    fn rejected(&self) -> Option<String> {
        match self.state {
            State::Open => None,
            State::Closed => Some("error:closed".into()),
            State::Poisoned => Some("error:poisoned".into()),
        }
    }

    fn poison(&mut self, reason: &str) -> String {
        self.pending.clear();
        self.state = State::Poisoned;
        format!("error:{reason}")
    }

    pub fn feed(&mut self, bytes: &[u8]) -> String {
        if let Some(error) = self.rejected() {
            return error;
        }
        let mut candidate = self.pending.clone();
        candidate.extend_from_slice(bytes);
        let mut consumed = 0;
        let mut frames = Vec::new();
        loop {
            match crate::frame::first(&candidate[consumed..]) {
                Ok(Some((payload, count))) => {
                    frames.push(crate::hex::encode(&payload));
                    consumed += count;
                }
                Ok(None) => break,
                Err(reason) => return self.poison(reason),
            }
        }
        self.pending = candidate[consumed..].to_vec();
        self.delivered += frames.len();
        format!("frames:{}", frames.join(","))
    }

    pub fn end(&mut self) -> String {
        if let Some(error) = self.rejected() {
            return error;
        }
        if !self.pending.is_empty() {
            return self.poison("truncated");
        }
        self.state = State::Closed;
        "closed".into()
    }

    pub fn status(&self) -> String {
        let state = match self.state {
            State::Open => "open",
            State::Closed => "closed",
            State::Poisoned => "poisoned",
        };
        format!(
            "pending:{} delivered:{} state:{state}",
            self.pending.len(),
            self.delivered
        )
    }
}
