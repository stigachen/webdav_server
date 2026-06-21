use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfig {
    pub enabled: bool,
    pub username: String,
    pub password: Option<String>,
    pub password_env: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            username: "davbox".to_string(),
            password: None,
            password_env: Some("DAVBOX_PASSWORD".to_string()),
        }
    }
}

impl AuthConfig {
    pub fn with_runtime_password(mut self, env: &[(String, String)]) -> Self {
        if !self.enabled {
            return self;
        }
        if self.password.is_none()
            && let Some(name) = &self.password_env
            && let Some((_, value)) = env.iter().find(|(key, _)| key == name)
        {
            self.password = Some(value.clone());
        }
        if self.password.is_none() {
            self.password = Some(generate_password());
        }
        self
    }
}

pub fn generate_password() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(42);
    let value = 10_000_000 + (nanos % 89_999_999);
    let text = value.to_string();
    format!("{}-{}", &text[..4], &text[4..])
}

pub fn basic_auth_matches(header: Option<&str>, auth: &AuthConfig) -> bool {
    if !auth.enabled {
        return true;
    }
    let Some(header) = header else {
        return false;
    };
    let Some(encoded) = header.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = base64_decode(encoded) else {
        return false;
    };
    let Ok(decoded) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((user, password)) = decoded.split_once(':') else {
        return false;
    };
    user == auth.username && Some(password) == auth.password.as_deref()
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("Invalid base64".to_string()),
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{AuthConfig, basic_auth_matches};

    #[test]
    fn validates_basic_auth() {
        let auth = AuthConfig {
            enabled: true,
            username: "davbox".to_string(),
            password: Some("secret".to_string()),
            password_env: None,
        };
        assert!(basic_auth_matches(
            Some("Basic ZGF2Ym94OnNlY3JldA=="),
            &auth
        ));
        assert!(!basic_auth_matches(Some("Basic ZGF2Ym94Ondyb25n"), &auth));
    }
}
