//! JSON-RPC 2.0 over standard input and output.
//!
//! Written out rather than pulled in. The protocol is four message shapes
//! and a framing rule, the gateway ships to companies whose IT will read its
//! dependency list, and every dependency here is one more thing that can
//! print to stdout.
//!
//! Which is the rule the whole module exists to keep: **stdout carries
//! protocol traffic and nothing else**. One stray `println!`, one library's
//! progress bar, one panic message, and the client sees a line that is not
//! JSON and drops the connection. The owner is shown "knowlith — failed"
//! with no explanation. So writing to stdout goes through [`Writer`] and
//! everything a developer wants to see goes to stderr.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{Value, json};

/// The error codes JSON-RPC defines. A tool that fails is *not* one of
/// these: a refusal the model should read about goes back as a successful
/// result carrying `isError`, because a protocol error is handled by the
/// client and never reaches the model that could have acted on it.
pub mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// One incoming message.
#[derive(Debug, Clone)]
pub struct Incoming {
    /// Absent for a notification, which must never be answered.
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

impl Incoming {
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// A named parameter, or `None` when it is missing or the wrong shape.
    pub fn param(&self, name: &str) -> Option<&Value> {
        self.params.get(name).filter(|v| !v.is_null())
    }

    pub fn string(&self, name: &str) -> Option<String> {
        self.param(name)?.as_str().map(str::to_string)
    }

    /// A string parameter that must be there and must not be blank.
    ///
    /// Blank counts as missing on purpose: a model that fills a required
    /// field with `""` has not answered the question, and searching for
    /// nothing would return the whole company.
    pub fn required(&self, name: &str) -> Result<String, String> {
        match self.string(name) {
            Some(value) if !value.trim().is_empty() => Ok(value),
            _ => Err(format!("\"{name}\" is required")),
        }
    }

    pub fn number(&self, name: &str) -> Option<u64> {
        self.param(name)?.as_u64()
    }
}

/// Parses one line. Returns `Err` with a code when the line is not a request
/// this server can answer at all.
pub fn parse(line: &str) -> Result<Incoming, (i64, String)> {
    let value: Value = serde_json::from_str(line)
        .map_err(|e| (code::PARSE_ERROR, format!("not JSON: {e}")))?;

    // Batches were removed from the protocol, but an older client may still
    // send one. Saying so is better than silence: silence looks like a hang.
    if value.is_array() {
        return Err((
            code::INVALID_REQUEST,
            "this server does not accept batched requests".to_string(),
        ));
    }

    let object = value
        .as_object()
        .ok_or((code::INVALID_REQUEST, "a request must be an object".to_string()))?;

    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or((code::INVALID_REQUEST, "no method".to_string()))?
        .to_string();

    Ok(Incoming {
        id: object.get("id").cloned().filter(|v| !v.is_null()),
        method,
        params: object.get("params").cloned().unwrap_or_else(|| json!({})),
    })
}

/// The one place anything is written to stdout.
///
/// Shared between the request loop and the thread that watches the lake for
/// approvals, so the lock is not an optimisation — two interleaved writes
/// would produce one unparseable line and end the session.
#[derive(Clone)]
pub struct Writer {
    out: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl Writer {
    pub fn stdout() -> Self {
        Self {
            out: Arc::new(Mutex::new(Box::new(std::io::stdout()))),
        }
    }

    pub fn to(sink: Box<dyn Write + Send>) -> Self {
        Self {
            out: Arc::new(Mutex::new(sink)),
        }
    }

    /// Writes one message, newline-terminated and flushed.
    ///
    /// A failure here means the client is gone. It is returned rather than
    /// logged, because the only correct response to a closed pipe is to stop
    /// — a server that keeps working for a client that has exited is a
    /// process the owner will find running next week.
    pub fn send(&self, message: &Value) -> std::io::Result<()> {
        let mut line = serde_json::to_string(message)
            .map_err(std::io::Error::other)?;
        // A message may not contain a newline; serde never emits one, and
        // this is the assertion that keeps that true if it ever does.
        debug_assert!(!line.contains('\n'));
        line.push('\n');

        let mut out = self.out.lock().map_err(|_| std::io::Error::other("writer poisoned"))?;
        out.write_all(line.as_bytes())?;
        out.flush()
    }

    pub fn result(&self, id: &Value, result: impl Serialize) -> std::io::Result<()> {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
    }

    pub fn error(&self, id: Option<&Value>, code: i64, message: &str) -> std::io::Result<()> {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id.cloned().unwrap_or(Value::Null),
            "error": { "code": code, "message": message },
        }))
    }

    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }
}

/// Reads newline-delimited messages from a stream.
pub struct Reader<R: Read> {
    inner: BufReader<R>,
}

impl<R: Read> Reader<R> {
    pub fn new(source: R) -> Self {
        Self {
            inner: BufReader::new(source),
        }
    }

    /// The next line, or `None` at end of input.
    ///
    /// Blank lines are skipped rather than reported: some clients write a
    /// trailing newline on shutdown, and answering it with a parse error
    /// makes the last thing in the log an error that was nobody's fault.
    pub fn next_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            let mut line = String::new();
            let read = self.inner.read_line(&mut line)?;
            if read == 0 {
                return Ok(None);
            }
            if line.trim().is_empty() {
                continue;
            }
            return Ok(Some(line));
        }
    }
}

/// Everything a developer needs to see, on the one stream that is safe.
pub fn log(message: &str) {
    let at = chrono::Utc::now().format("%H:%M:%S");
    let _ = writeln!(std::io::stderr(), "[knowlith {at}] {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_parsed() {
        let message = parse(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        assert_eq!(message.method, "tools/list");
        assert!(!message.is_notification());
    }

    #[test]
    fn a_notification_has_no_id_and_is_never_answered() {
        let message = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert!(message.is_notification());
    }

    #[test]
    fn a_null_id_counts_as_a_notification() {
        let message = parse(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#).unwrap();
        assert!(message.is_notification());
    }

    #[test]
    fn a_batch_is_refused_with_a_reason() {
        let (code, message) = parse("[{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}]").unwrap_err();
        assert_eq!(code, code::INVALID_REQUEST);
        assert!(message.contains("batched"));
    }

    #[test]
    fn a_broken_line_is_a_parse_error_not_a_crash() {
        let (code, _) = parse("{not json").unwrap_err();
        assert_eq!(code, code::PARSE_ERROR);
    }

    #[test]
    fn a_blank_required_parameter_is_a_missing_one() {
        let message = parse(r#"{"jsonrpc":"2.0","id":1,"method":"x","params":{"question":"   "}}"#).unwrap();
        assert!(message.required("question").is_err());
    }

    #[test]
    fn messages_are_one_line_each() {
        let sink = SharedSink::default();
        let writer = Writer::to(Box::new(sink.clone()));
        writer
            .result(&json!(1), json!({ "text": "two\nlines" }))
            .unwrap();

        let written = sink.text();
        assert_eq!(written.lines().count(), 1, "{written}");
        assert!(written.ends_with('\n'));
        assert!(written.contains("two\\nlines"), "the newline was escaped, not emitted");
    }

    #[test]
    fn blank_lines_between_messages_are_skipped() {
        let input = "\n\n{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\n";
        let mut reader = Reader::new(input.as_bytes());
        let line = reader.next_line().unwrap().unwrap();
        assert!(line.contains("ping"));
        assert!(reader.next_line().unwrap().is_none());
    }

    /// A stdout stand-in the tests can read back.
    #[derive(Clone, Default)]
    pub struct SharedSink(Arc<Mutex<Vec<u8>>>);

    impl SharedSink {
        pub fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
