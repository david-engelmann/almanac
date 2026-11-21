//! A minimal server-sent events reader.
//!
//! A Streamable HTTP server may answer a POST with an SSE stream
//! carrying notifications and then the JSON-RPC response. The probe
//! needs the messages, not the stream, so this module reduces a
//! stream body to the JSON values its `data:` fields carried.

/// Parse an SSE body into the JSON values of its message events.
/// Events that carry no data, or data that is not JSON, are
/// skipped. A checker reads what a server sent, and a malformed
/// event is the server's problem to be reported by the caller when
/// the expected response never appears.
pub fn messages(body: &str) -> Vec<serde_json::Value> {
    // The format's canonical line ending is CRLF, and a body
    // framed that way separates its events with \r\n\r\n, which
    // an LF-only split never matches. Normalise first, so both
    // framings parse.
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    for event in normalized.split("\n\n") {
        let mut data = String::new();
        for line in event.lines() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if !data.is_empty()
            && let Ok(value) = serde_json::from_str(&data)
        {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_data_messages_in_order() {
        let body = "event: message\ndata: {\"id\":1}\n\nevent: message\ndata: {\"id\":2}\n\n";
        let values = messages(body);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["id"], 1);
        assert_eq!(values[1]["id"], 2);
    }

    #[test]
    fn ignores_comments_and_empty_events() {
        let body = ": ping\n\ndata: {\"ok\":true}\n\n";
        assert_eq!(messages(body).len(), 1);
    }

    #[test]
    fn parses_crlf_framed_events() {
        let body =
            "event: message\r\ndata: {\"id\":1}\r\n\r\nevent: message\r\ndata: {\"id\":2}\r\n\r\n";
        let values = messages(body);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["id"], 1);
        assert_eq!(values[1]["id"], 2);
    }
}
