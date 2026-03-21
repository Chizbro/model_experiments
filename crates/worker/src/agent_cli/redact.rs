//! Remove known secrets before logging or forwarding log lines.

/// Replace every occurrence of each secret substring with a fixed placeholder.
pub fn redact_secrets<'a>(line: &'a str, secrets: &[String]) -> std::borrow::Cow<'a, str> {
    if secrets.is_empty() || line.is_empty() {
        return std::borrow::Cow::Borrowed(line);
    }
    let mut out = line.to_string();
    let mut any = false;
    for s in secrets {
        if s.is_empty() {
            continue;
        }
        if out.contains(s.as_str()) {
            any = true;
            out = out.replace(s.as_str(), "[REDACTED]");
        }
    }
    if any {
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(line)
    }
}

/// Holds secret material for repeated redaction (e.g. agent token).
#[derive(Debug, Clone, Default)]
pub struct LogRedactor {
    secrets: Vec<String>,
}

impl LogRedactor {
    pub fn new(secrets: Vec<String>) -> Self {
        Self { secrets }
    }

    pub fn redact<'a>(&self, line: &'a str) -> std::borrow::Cow<'a, str> {
        redact_secrets(line, &self.secrets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_token() {
        let t = "super-secret-token".to_string();
        let line = format!("error: auth failed token={t} end");
        let r = redact_secrets(&line, &[t.clone()]);
        assert!(!r.contains("super-secret"));
        assert!(r.contains("[REDACTED]"));
    }
}
