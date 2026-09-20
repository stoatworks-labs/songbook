//! OAuth 2.0 authorization-code flow with PKCE for a desktop app: open the
//! provider's consent page in the system browser, catch the redirect on a
//! loopback listener, exchange the code for tokens. No client secret is
//! needed for Dropbox or Microsoft; Google's "desktop" client type carries
//! one that Google itself documents as not confidential.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use base64::Engine;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix seconds when `access_token` expires, if the provider said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub account: String,
}

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: String,
    /// Extra query parameters on the authorize URL (`token_access_type=offline`, `access_type=offline`, …).
    pub extra_authorize: Vec<(String, String)>,
}

pub struct PendingAuth {
    pub url: String,
    pub state: String,
    pub verifier: String,
    pub redirect_uri: String,
    listener: TcpListener,
    config: OAuthConfig,
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Bind a loopback port and build the consent URL. The caller opens
/// `url` in a browser, then calls [`PendingAuth::wait`].
pub fn begin(config: OAuthConfig) -> Result<PendingAuth> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let mut raw = [0u8; 48];
    rand::rng().fill_bytes(&mut raw);
    let verifier = b64url(&raw);
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    let mut st = [0u8; 16];
    rand::rng().fill_bytes(&mut st);
    let state = b64url(&st);
    let mut url = url::Url::parse(&config.authorize_url).map_err(|e| Error::Other(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &config.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &config.scope)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        for (k, v) in &config.extra_authorize {
            q.append_pair(k, v);
        }
    }
    Ok(PendingAuth { url: url.to_string(), state, verifier, redirect_uri, listener, config })
}

impl PendingAuth {
    /// Block until the browser is redirected back (or `timeout` passes),
    /// then exchange the code for tokens.
    pub fn wait(self, timeout: Duration) -> Result<Tokens> {
        self.listener.set_nonblocking(true)?;
        let start = std::time::Instant::now();
        let code = loop {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let line = req.lines().next().unwrap_or("").to_string();
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let parsed = url::Url::parse(&format!("http://127.0.0.1{path}")).ok();
                    let mut code = None;
                    let mut ok_state = false;
                    let mut err = None;
                    if let Some(u) = parsed {
                        for (k, v) in u.query_pairs() {
                            match k.as_ref() {
                                "code" => code = Some(v.to_string()),
                                "state" => ok_state = v == self.state,
                                "error" => err = Some(v.to_string()),
                                _ => {}
                            }
                        }
                    }
                    let (status, body) = match (&code, ok_state, &err) {
                        (Some(_), true, _) => ("200 OK", "<html><body style=\"font-family:sans-serif\"><h2>Songbook is connected.</h2><p>You can close this tab.</p></body></html>"),
                        (_, _, Some(_)) => ("400 Bad Request", "<html><body style=\"font-family:sans-serif\"><h2>Sign-in was refused.</h2></body></html>"),
                        _ => ("400 Bad Request", "<html><body style=\"font-family:sans-serif\"><h2>Unexpected reply.</h2></body></html>"),
                    };
                    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = stream.flush();
                    if let Some(e) = err {
                        return Err(Error::Other(format!("authorization refused: {e}")));
                    }
                    if let (Some(c), true) = (code, ok_state) {
                        break c;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > timeout {
                        return Err(Error::Other("timed out waiting for the browser".into()));
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(e.into()),
            }
        };
        exchange(&self.config, &code, &self.verifier, &self.redirect_uri)
    }
}

fn exchange(config: &OAuthConfig, code: &str, verifier: &str, redirect_uri: &str) -> Result<Tokens> {
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", &config.client_id),
        ("code_verifier", verifier),
    ];
    if let Some(s) = &config.client_secret {
        form.push(("client_secret", s));
    }
    let mut resp = ureq::post(&config.token_url).send_form(form).map_err(|e| Error::Other(format!("token exchange: {e}")))?;
    tokens_from(resp.body_mut().read_json().map_err(|e| Error::Other(e.to_string()))?)
}

pub fn refresh(config: &OAuthConfig, refresh_token: &str) -> Result<Tokens> {
    let mut form: Vec<(&str, &str)> = vec![("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", &config.client_id)];
    if let Some(s) = &config.client_secret {
        form.push(("client_secret", s));
    }
    let mut resp = ureq::post(&config.token_url).send_form(form).map_err(|e| Error::Other(format!("token refresh: {e}")))?;
    let mut t = tokens_from(resp.body_mut().read_json().map_err(|e| Error::Other(e.to_string()))?)?;
    if t.refresh_token.is_none() {
        t.refresh_token = Some(refresh_token.to_string());
    }
    Ok(t)
}

fn tokens_from(v: serde_json::Value) -> Result<Tokens> {
    let access = v.get("access_token").and_then(|x| x.as_str()).ok_or_else(|| Error::Other(format!("no access_token in {v}")))?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    Ok(Tokens {
        access_token: access.to_string(),
        refresh_token: v.get("refresh_token").and_then(|x| x.as_str()).map(str::to_string),
        expires_at: v.get("expires_in").and_then(|x| x.as_u64()).map(|s| now + s),
        scope: v.get("scope").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        account: v.get("account_id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_url_carries_pkce() {
        let p = begin(OAuthConfig {
            authorize_url: "https://www.dropbox.com/oauth2/authorize".into(),
            token_url: "https://api.dropboxapi.com/oauth2/token".into(),
            client_id: "abc".into(),
            client_secret: None,
            scope: "files.content.write files.content.read".into(),
            extra_authorize: vec![("token_access_type".into(), "offline".into())],
        })
        .unwrap();
        assert!(p.url.contains("code_challenge_method=S256"));
        assert!(p.url.contains("client_id=abc"));
        assert!(p.url.contains("token_access_type=offline"));
        assert!(p.redirect_uri.starts_with("http://127.0.0.1:"));
        assert!(p.verifier.len() >= 43);
    }
}
