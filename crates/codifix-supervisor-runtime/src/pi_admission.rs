//! Admission for the pinned Pi RPC client's first, tool-free request. This is
//! deliberately not a generic OpenAI proxy: only the already prepared Memory
//! request can be sent upstream. Pi supplies neither authority nor provider
//! credentials, and cannot select model options, extra context or endpoints.

use crate::{model::PreparedRequest, strict_json};
use chrono::NaiveDate;
use serde_json::json;

/// Includes JSON escaping of the bounded question/evidence, including the inner
/// JSON text carried as Pi's single user message. Check before parsing.
pub const MAX_PI_REQUEST_BYTES: usize = 262_144;
const DATE_PREFIX: &str = "\nCurrent date and time: ";
const DATE_FORMAT: &str = "%A, %B %-d, %Y";

/// Borrow the immutable, caller-reviewed request. This is not a disclosure
/// capability, HTTP authentication, a lease, or budget admission. No Debug or
/// serialization: these strings may contain private reviewed knowledge.
pub struct PiAdmission<'a> {
    request: &'a PreparedRequest,
}

impl<'a> PiAdmission<'a> {
    pub fn new(request: &'a PreparedRequest) -> Self {
        Self { request }
    }

    /// Supply to the isolated Pi process as its only system prompt.
    pub fn system_prompt(&self) -> &'a str {
        self.request.generation_body()["instructions"]
            .as_str()
            .expect("PreparedRequest constructs fixed instructions")
    }

    /// Supply as the single RPC prompt, not a user-selected file or history.
    pub fn prompt(&self) -> &'a str {
        self.request.generation_body()["input"][0]["content"][0]["text"]
            .as_str()
            .expect("PreparedRequest constructs one text input")
    }

    /// Validate bytes from Pi BEFORE counting tokens or touching admission state.
    /// Unknown fields, null safety fields and duplicate keys fail closed. The
    /// pinned app appends a date even to an explicit system prompt; accept only
    /// its canonical English date syntax, then discard it. It never affects the
    /// provider payload, semantic clock, lease, policy or accounting.
    ///
    /// A successful check authorizes no network effect on its own. The trusted
    /// broker must send `request`, not forward these bytes: Memory fixes low
    /// reasoning, structured output, no storage/tools and non-streaming upstream
    /// independently of Pi's streaming/off-thinking client profile.
    pub fn validate(&self, wire: &[u8]) -> Result<(), PiAdmissionError> {
        if wire.is_empty() || wire.len() > MAX_PI_REQUEST_BYTES {
            return Err(PiAdmissionError::BodyBounds);
        }
        let mut body = strict_json::parse(wire).map_err(|_| PiAdmissionError::Malformed)?;
        let fields = body.as_object_mut().ok_or(PiAdmissionError::Malformed)?;
        if let Some(tools) = fields.remove("tools")
            && tools != json!([])
        {
            return Err(PiAdmissionError::ProfileMismatch);
        }
        let instructions = fields
            .get("instructions")
            .and_then(|v| v.as_str())
            .ok_or(PiAdmissionError::ProfileMismatch)?;
        let date = instructions
            .strip_prefix(self.system_prompt())
            .and_then(|s| s.strip_prefix(DATE_PREFIX))
            .filter(|s| s.len() <= 64 && s.is_ascii())
            .ok_or(PiAdmissionError::ProfileMismatch)?;
        let parsed = NaiveDate::parse_from_str(date, DATE_FORMAT)
            .map_err(|_| PiAdmissionError::ProfileMismatch)?;
        if parsed.format(DATE_FORMAT).to_string() != date {
            return Err(PiAdmissionError::ProfileMismatch);
        }
        let expected = json!({
            "model": self.request.generation_body()["model"],
            "input": self.request.generation_body()["input"],
            "instructions": instructions,
            "max_output_tokens": self.request.generation_body()["max_output_tokens"],
            "stream": true,
            "store": false
        });
        if body != expected {
            return Err(PiAdmissionError::ProfileMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PiAdmissionError {
    #[error("Pi request exceeds the admitted body bounds")]
    BodyBounds,
    #[error("Pi request is malformed or contains duplicate fields")]
    Malformed,
    #[error("Pi request does not match the approved input and fixed profile")]
    ProfileMismatch,
}
