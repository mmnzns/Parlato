// Parlato : what a failed dictation may keep in History.
//
// A provider's error text can carry its response body, a URL or other data
// (even the dictated words). History keeps only a short code in the stable
// `PARLA_ERR:<code>[:<arg>]` format that the frontend translates
// (`src/lib/translateError.ts`). Never store the raw message.

const PREFIX: &str = "PARLA_ERR:";
const MAX_CODE_LEN: usize = 80;

/// Short stored code for an error message.
/// - A message that already is a `PARLA_ERR:` code is kept (cut at the first
///   character that is not part of a code).
/// - `timeout...` -> `PARLA_ERR:providerTimeout`
/// - `network_error...` -> `PARLA_ERR:providerNetwork`
/// - `HTTP <status>...` -> `PARLA_ERR:providerHttp:<status>`
/// - anything else -> `PARLA_ERR:transcriptionFailed`
pub fn storable_error_code(err: &str) -> String {
    let err = err.trim_start();
    if let Some(rest) = err.strip_prefix(PREFIX) {
        let code: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '-'))
            .take(MAX_CODE_LEN)
            .collect();
        let code = code.trim_end_matches(':');
        if !code.is_empty() {
            return format!("{PREFIX}{code}");
        }
    }
    if err.starts_with("timeout") {
        return format!("{PREFIX}providerTimeout");
    }
    if err.starts_with("network_error") {
        return format!("{PREFIX}providerNetwork");
    }
    if let Some(rest) = err.strip_prefix("HTTP ") {
        let status: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if status.len() == 3 {
            return format!("{PREFIX}providerHttp:{status}");
        }
    }
    format!("{PREFIX}transcriptionFailed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_stable_codes() {
        assert_eq!(storable_error_code("PARLA_ERR:noWhisper"), "PARLA_ERR:noWhisper");
        assert_eq!(
            storable_error_code("PARLA_ERR:apiKey:groq"),
            "PARLA_ERR:apiKey:groq"
        );
    }

    #[test]
    fn cuts_anything_after_the_code() {
        assert_eq!(
            storable_error_code("PARLA_ERR:noWhisper: the dictated words were here"),
            "PARLA_ERR:noWhisper"
        );
    }

    #[test]
    fn provider_body_is_never_kept() {
        let body = "HTTP 400 Bad Request: {\"error\":\"could not process: my secret sentence\"}";
        let code = storable_error_code(body);
        assert_eq!(code, "PARLA_ERR:providerHttp:400");
        assert!(!code.contains("secret"));
        let code = storable_error_code("http: error decoding response body: my secret sentence");
        assert_eq!(code, "PARLA_ERR:transcriptionFailed");
    }

    #[test]
    fn classifies_timeout_and_network() {
        assert_eq!(
            storable_error_code("timeout: operation timed out"),
            "PARLA_ERR:providerTimeout"
        );
        assert_eq!(
            storable_error_code("network_error: connection refused"),
            "PARLA_ERR:providerNetwork"
        );
    }

    #[test]
    fn unknown_text_becomes_generic() {
        assert_eq!(
            storable_error_code("parse JSON Deepgram: expected value"),
            "PARLA_ERR:transcriptionFailed"
        );
        assert_eq!(storable_error_code("PARLA_ERR:"), "PARLA_ERR:transcriptionFailed");
        assert_eq!(storable_error_code(""), "PARLA_ERR:transcriptionFailed");
    }
}
