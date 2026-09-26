use std::net::Ipv4Addr;

use http::{HeaderMap, Method};

#[derive(Clone, Default)]
pub struct DashboardOrigin {
    public: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "PUBLIC_BASE_URL must be one absolute http(s) root URL without credentials, a path, query, or fragment"
)]
pub struct InvalidOrigin;

impl DashboardOrigin {
    pub fn new(value: &str) -> Result<Self, InvalidOrigin> {
        Ok(Self {
            public: if value.is_empty() {
                None
            } else {
                Some(Self::parse(value)?)
            },
        })
    }

    fn parse(value: &str) -> Result<String, InvalidOrigin> {
        if value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "\\%?#@".contains(c))
        {
            return Err(InvalidOrigin);
        }
        let (scheme, rest) = value.split_once("://").ok_or(InvalidOrigin)?;
        if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return Err(InvalidOrigin);
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.is_empty() || !path.is_empty() || authority.ends_with(':') {
            return Err(InvalidOrigin);
        }
        let host = if authority.starts_with('[') {
            let (host, suffix) = authority.split_once(']').ok_or(InvalidOrigin)?;
            if !suffix.is_empty()
                && (!suffix.starts_with(':') || !suffix[1..].bytes().all(|b| b.is_ascii_digit()))
            {
                return Err(InvalidOrigin);
            }
            host
        } else {
            let (host, port) = authority.split_once(':').unwrap_or((authority, ""));
            if !port.bytes().all(|b| b.is_ascii_digit()) {
                return Err(InvalidOrigin);
            }
            let last = host
                .trim_end_matches('.')
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if last.bytes().all(|b| b.is_ascii_digit())
                || last.starts_with("0x") && last[2..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                host.trim_end_matches('.')
                    .parse::<Ipv4Addr>()
                    .map_err(|_| InvalidOrigin)?;
            }
            host
        };
        if host.is_empty() {
            return Err(InvalidOrigin);
        }
        let url = url::Url::parse(value).map_err(|_| InvalidOrigin)?;
        let host = url.host_str().ok_or(InvalidOrigin)?;
        if !host.starts_with('[')
            && !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || url.port_or_known_default().is_none_or(|port| port == 0)
        {
            return Err(InvalidOrigin);
        }
        Ok(url.origin().ascii_serialization())
    }

    pub fn expected(&self, headers: &HeaderMap, secure_transport: bool) -> Option<String> {
        self.public.clone().or_else(|| {
            if headers.get_all("host").iter().count() != 1 {
                return None;
            }
            let host = headers.get("host")?.to_str().ok()?;
            Self::parse(&format!(
                "{}://{host}",
                if secure_transport { "https" } else { "http" }
            ))
            .ok()
        })
    }

    pub fn secure_cookie(&self, secure_transport: bool) -> bool {
        self.public
            .as_ref()
            .map(|url| url.starts_with("https://"))
            .unwrap_or(secure_transport)
    }

    pub fn permits(
        &self,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
        secure_transport: bool,
    ) -> bool {
        let mutation = !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
        let socket = path.starts_with("/socket.io");
        if !mutation && !socket {
            return true;
        }
        let origin_ok = !headers.contains_key("origin")
            || headers.get_all("origin").iter().count() == 1
                && headers
                    .get("origin")
                    .and_then(|v| v.to_str().ok())
                    .zip(self.expected(headers, secure_transport))
                    .is_some_and(|(origin, expected)| origin == expected);
        origin_ok
            && headers
                .get("sec-fetch-site")
                .is_none_or(|v| v != "cross-site")
            && (!mutation || socket || headers.get("x-tdm-request").is_some_and(|v| v == "1"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_origins_use_browser_canonicalization() {
        for (input, expected) in [
            (
                "HTTPS://Drops.Example.Com:443/",
                "https://drops.example.com",
            ),
            ("http://localhost:80/", "http://localhost"),
            ("http://192.168.1.2.:8080/", "http://192.168.1.2:8080"),
            ("https://[2001:0DB8:0:0:0:0:0:1]/", "https://[2001:db8::1]"),
            ("https://[::ffff:192.0.2.1]/", "https://[::ffff:c000:201]"),
            ("https://münich.example/", "https://xn--mnich-kva.example"),
        ] {
            let origin = DashboardOrigin::new(input).unwrap();
            assert_eq!(origin.expected(&HeaderMap::new(), false).unwrap(), expected);
            assert_eq!(
                origin.secure_cookie(false),
                expected.starts_with("https://")
            );
        }
    }

    #[test]
    fn ambiguous_or_secret_containing_origins_fail_without_echoing_input() {
        for value in [
            " ",
            "drops.example.com",
            "//drops.example.com",
            "https://",
            "https:///drops.example.com",
            "ftp://drops.example.com",
            "wss://drops.example.com",
            "https://*.example.com",
            "https://drops.example.com/path",
            "https://drops.example.com/.",
            "https://drops.example.com//",
            "https://drops.example.com?",
            "https://drops.example.com#",
            "https://user:secret@drops.example.com",
            "https://user@drops.example.com",
            "https://@drops.example.com",
            "https://drops.example.com:",
            "https://drops.example.com:0",
            "https://drops.example.com:65536",
            "https://drops.example.com:abc",
            "https://drops.example.com:١٢٣",
            "https://[::1]suffix",
            "https://[::1",
            "https://[not-ipv6]",
            "https://drops.example.com,https://other.example.com",
            "https://drops.example.com other.example.com",
            " https://drops.example.com",
            "https://drops.example.com\n",
            "https://drop\ts.example.com",
            "https://drops.example.com\0",
            "https://drops.example.com\\evil",
            "https://drops%2eexample.com",
            "https://drops.example.com/%2e",
            "http://127.1",
            "http://0177.0.0.1",
            "http://0x7f.0.0.1",
            "http://2130706433",
            "http://1.2.3.256",
            "http://example.123",
            "http://0x7f000001",
            "http://1.2.3.0x",
            "https://a\u{200c}b.example",
            "https://a\u{200d}b.example",
        ] {
            let error = DashboardOrigin::new(value)
                .err()
                .unwrap_or_else(|| panic!("accepted {value}"));
            assert!(error.to_string().starts_with("PUBLIC_BASE_URL must be"));
            assert!(!error.to_string().contains("secret"));
        }
    }

    #[test]
    fn public_origin_does_not_trust_forwarded_host_or_bypass_csrf() {
        let policy = DashboardOrigin::new("https://drops.example.com").unwrap();
        let mut headers = HeaderMap::from_iter([
            (http::header::HOST, "backend:8080".parse().unwrap()),
            (
                http::header::ORIGIN,
                "https://drops.example.com".parse().unwrap(),
            ),
        ]);
        assert!(policy.permits(&Method::GET, "/socket.io/", &headers, false));
        assert!(!policy.permits(&Method::POST, "/api/settings", &headers, false));
        headers.insert("x-tdm-request", "1".parse().unwrap());
        assert!(policy.permits(&Method::POST, "/api/settings", &headers, false));
        headers.insert("x-forwarded-host", "foreign.example".parse().unwrap());
        headers.insert("origin", "https://foreign.example".parse().unwrap());
        assert!(!policy.permits(&Method::POST, "/api/settings", &headers, false));
        assert!(!policy.permits(&Method::GET, "/socket.io/", &headers, false));
        headers.remove("origin");
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(!policy.permits(&Method::POST, "/socket.io/", &headers, false));
        headers.remove("sec-fetch-site");
        assert!(policy.permits(&Method::POST, "/socket.io/", &headers, false));
        assert_eq!(
            DashboardOrigin::default()
                .expected(&headers, false)
                .as_deref(),
            Some("http://backend:8080")
        );
    }
}
