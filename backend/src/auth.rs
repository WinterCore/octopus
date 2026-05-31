use std::{collections::HashMap, sync::Mutex, time::{Duration, Instant}};

use hyper::{header, Request};
use rand::RngCore;
use subtle::ConstantTimeEq;

const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24);
pub const COOKIE_NAME: &str = "octopus_admin";

pub struct AuthState {
    password: String,
    sessions: Mutex<HashMap<String, Instant>>,
}

impl AuthState {
    pub fn new(password: String) -> Self {
        Self {
            password,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn check_password(&self, candidate: &str) -> bool {
        let a = candidate.as_bytes();
        let b = self.password.as_bytes();
        if a.len() != b.len() {
            // Still do a fixed-length compare to avoid trivial length-based timing leaks.
            let _ = a.ct_eq(a);
            return false;
        }
        a.ct_eq(b).into()
    }

    pub fn issue_session(&self) -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = hex(&bytes);
        let mut sessions = self.sessions.lock().expect("session lock");
        sessions.insert(token.clone(), Instant::now());
        token
    }

    pub fn revoke(&self, token: &str) {
        let mut sessions = self.sessions.lock().expect("session lock");
        sessions.remove(token);
    }

    pub fn validate(&self, token: &str) -> bool {
        let mut sessions = self.sessions.lock().expect("session lock");
        match sessions.get(token).copied() {
            Some(issued) if issued.elapsed() < SESSION_TTL => true,
            Some(_) => {
                sessions.remove(token);
                false
            }
            None => false,
        }
    }
}

pub fn extract_token<B>(req: &Request<B>) -> Option<String> {
    let cookie_header = req.headers().get(header::COOKIE)?.to_str().ok()?;
    for kv in cookie_header.split(';') {
        let kv = kv.trim();
        if let Some(rest) = kv.strip_prefix(&format!("{}=", COOKIE_NAME)) {
            return Some(rest.to_string());
        }
    }
    None
}

pub fn cookie_header_value(token: &str) -> String {
    format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        COOKIE_NAME,
        token,
        SESSION_TTL.as_secs()
    )
}

pub fn cookie_clear_value() -> String {
    format!(
        "{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        COOKIE_NAME
    )
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
