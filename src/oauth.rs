use serde::Deserialize;

const GOOGLE_DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar https://www.googleapis.com/auth/userinfo.email";

fn client_id() -> Option<&'static str> {
    option_env!("GOOGLE_CLIENT_ID")
}

fn client_secret() -> Option<&'static str> {
    option_env!("GOOGLE_CLIENT_SECRET")
}

pub fn credentials_configured() -> bool {
    client_id().is_some() && client_secret().is_some()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    email: String,
}

pub fn request_device_code() -> Result<DeviceCodeResponse, String> {
    let id = client_id().ok_or("GOOGLE_CLIENT_ID not configured")?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(GOOGLE_DEVICE_CODE_URL)
        .form(&[("client_id", id), ("scope", CALENDAR_SCOPE)])
        .send()
        .map_err(|e| format!("Device code request failed: {e}"))?;

    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Failed to read response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Device code request failed ({}): {}", status, body));
    }

    serde_json::from_str(&body).map_err(|e| format!("Failed to parse device code response: {e}"))
}

/// Poll for token. Returns Ok(None) if authorization is still pending,
/// Ok(Some(token)) on success, Err on permanent failure.
pub fn poll_token(device_code: &str) -> Result<Option<TokenResponse>, String> {
    let id = client_id().ok_or("GOOGLE_CLIENT_ID not configured")?;
    let secret = client_secret().ok_or("GOOGLE_CLIENT_SECRET not configured")?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", id),
            ("client_secret", secret),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .map_err(|e| format!("Token poll failed: {e}"))?;

    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Failed to read response: {e}"))?;

    if status.is_success() {
        let token: TokenResponse =
            serde_json::from_str(&body).map_err(|e| format!("Failed to parse token: {e}"))?;
        return Ok(Some(token));
    }

    // Check if it's a pending/slow_down error (non-permanent)
    if let Ok(err) = serde_json::from_str::<TokenErrorResponse>(&body) {
        match err.error.as_str() {
            "authorization_pending" | "slow_down" => return Ok(None),
            "expired_token" => return Err("Device code expired — please try again".into()),
            "access_denied" => return Err("Access denied by user".into()),
            other => return Err(format!("OAuth error: {other}")),
        }
    }

    Err(format!("Token request failed ({}): {}", status, body))
}

pub fn refresh_access_token(refresh_token: &str) -> Result<TokenResponse, String> {
    let id = client_id().ok_or("GOOGLE_CLIENT_ID not configured")?;
    let secret = client_secret().ok_or("GOOGLE_CLIENT_SECRET not configured")?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", id),
            ("client_secret", secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Failed to read response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Token refresh failed ({}): {}", status, body));
    }

    serde_json::from_str(&body).map_err(|e| format!("Failed to parse refresh response: {e}"))
}

pub fn fetch_user_email(access_token: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("User info request failed: {e}"))?;

    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Failed to read response: {e}"))?;

    if !status.is_success() {
        return Err(format!("User info request failed ({}): {}", status, body));
    }

    let info: UserInfoResponse =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse user info: {e}"))?;
    Ok(info.email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_response_parses() {
        let json = r#"{
            "device_code": "abc123",
            "user_code": "ABCD-EFGH",
            "verification_url": "https://www.google.com/device",
            "expires_in": 1800,
            "interval": 5
        }"#;
        let resp: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.device_code, "abc123");
        assert_eq!(resp.user_code, "ABCD-EFGH");
        assert_eq!(resp.verification_url, "https://www.google.com/device");
        assert_eq!(resp.expires_in, 1800);
        assert_eq!(resp.interval, 5);
    }

    #[test]
    fn token_response_parses() {
        let json = r#"{
            "access_token": "ya29.xxx",
            "refresh_token": "1//xxx",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;
        let resp: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "ya29.xxx");
        assert_eq!(resp.refresh_token.as_deref(), Some("1//xxx"));
        assert_eq!(resp.expires_in, 3600);
    }

    #[test]
    fn token_response_without_refresh_token() {
        let json = r#"{
            "access_token": "ya29.xxx",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;
        let resp: TokenResponse = serde_json::from_str(json).unwrap();
        assert!(resp.refresh_token.is_none());
    }

    #[test]
    fn token_error_response_parses() {
        let json = r#"{"error": "authorization_pending"}"#;
        let resp: TokenErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, "authorization_pending");
    }

    #[test]
    fn user_info_response_parses() {
        let json = r#"{"email": "user@gmail.com", "id": "123"}"#;
        let resp: UserInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.email, "user@gmail.com");
    }

    #[test]
    fn credentials_configured_returns_false_without_env() {
        // In test environment, GOOGLE_CLIENT_ID/SECRET are typically not set at compile time
        // This test just verifies the function doesn't panic
        let _ = credentials_configured();
    }
}
