//! Pairing: signing in a device through a browser, for any kind of sign-in.
//!
//! The device starts a pairing and shows its short code. Someone signed in on
//! the web approves that code, and the device, polling with its secret, then
//! gets a session. Everything here lives in memory and lasts ten minutes, like
//! sign-ins in progress: a restart just means starting again.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// How long a pairing waits to be approved and collected.
pub const WAIT: Duration = Duration::from_secs(10 * 60);
/// How often a device should poll.
pub const POLL_SECONDS: u64 = 5;

/// Starting is public, so it's limited overall, and memory with it.
const STARTS_PER_MINUTE: usize = 30;
const MAX_WAITING: usize = 200;
/// Wrong codes one user may enter in `WAIT`, which keeps guessing out of reach.
const MAX_MISSES: usize = 10;

/// Consonants that are hard to confuse, as in RFC 8628; no words, no 0/O.
const CODE_LETTERS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
const CODE_LENGTH: usize = 8;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    #[error("Choose a device name of 1 to 64 characters.")]
    BadName,
    #[error("Too many devices are pairing right now. Try again in a minute.")]
    TooManyStarts,
    #[error("That code doesn't match a device waiting to pair. Check it and try again.")]
    UnknownCode,
    #[error("Too many wrong codes. Try again in a few minutes.")]
    TooManyMisses,
}

/// What a device needs to carry on after starting.
#[derive(Debug, Clone)]
pub struct Started {
    /// For the person to approve; shown as `XXXX-XXXX`.
    pub code: String,
    /// For the device alone, to collect its session.
    pub secret: String,
}

/// Where a pairing has got to, as its device sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    Waiting,
    /// Approved by this user; the device gets its session now, once.
    Approved {
        user: i64,
        device_name: String,
    },
    Declined,
    /// Unknown, expired or already collected.
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Waiting,
    Approved(i64),
    Declined,
}

struct Pairing {
    secret_hash: String,
    device_name: String,
    started: Instant,
    answer: Answer,
}

#[derive(Default)]
struct Inner {
    by_code: HashMap<String, Pairing>,
    recent_starts: VecDeque<Instant>,
    misses: HashMap<i64, VecDeque<Instant>>,
}

/// Pairings in progress. Cloning shares them.
#[derive(Clone)]
pub struct Pairings {
    inner: Arc<Mutex<Inner>>,
    wait: Duration,
}

impl Default for Pairings {
    fn default() -> Self {
        Pairings::with_wait(WAIT)
    }
}

fn hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

fn new_code() -> String {
    let mut bytes = [0u8; CODE_LENGTH];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    // 256 isn't a multiple of 20, so the first letters come up slightly more
    // often, which doesn't matter given the limits on guessing.
    bytes
        .iter()
        .map(|b| CODE_LETTERS[*b as usize % CODE_LETTERS.len()] as char)
        .collect()
}

/// `xxxx xxxx` or `XXXX-XXXX` as stored: letters only, upper case.
fn normalise(code: &str) -> String {
    code.chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

impl Pairings {
    pub fn with_wait(wait: Duration) -> Self {
        Pairings {
            inner: Arc::default(),
            wait,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        let mut inner = self.inner.lock().unwrap();
        let wait = self.wait;
        inner.by_code.retain(|_, p| p.started.elapsed() < wait);
        inner
            .misses
            .retain(|_, times| times.iter().any(|t| t.elapsed() < WAIT));
        while inner
            .recent_starts
            .front()
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(60))
        {
            inner.recent_starts.pop_front();
        }
        inner
    }

    /// Starts a pairing for a device of this name.
    pub fn start(&self, device_name: &str) -> Result<Started, PairError> {
        let device_name =
            crate::accounts::check_device_name(device_name).map_err(|_| PairError::BadName)?;
        let mut inner = self.lock();
        if inner.recent_starts.len() >= STARTS_PER_MINUTE || inner.by_code.len() >= MAX_WAITING {
            return Err(PairError::TooManyStarts);
        }
        inner.recent_starts.push_back(Instant::now());
        let mut code = new_code();
        while inner.by_code.contains_key(&code) {
            code = new_code();
        }
        let secret = crate::apikeys::random_secret().replacen("mc_", "", 1);
        inner.by_code.insert(
            code.clone(),
            Pairing {
                secret_hash: hash(&secret),
                device_name,
                started: Instant::now(),
                answer: Answer::Waiting,
            },
        );
        Ok(Started {
            code: format!("{}-{}", &code[..4], &code[4..]),
            secret,
        })
    }

    /// The name of the device waiting behind a code, for `user` to check before
    /// answering. Wrong codes count against them.
    pub fn device_name(&self, user: i64, code: &str) -> Result<String, PairError> {
        let mut inner = self.lock();
        let found = find_waiting(&mut inner, user, code)?;
        Ok(found.device_name.clone())
    }

    /// Approves or declines the device behind a code. Returns its name.
    pub fn answer(&self, user: i64, code: &str, approve: bool) -> Result<String, PairError> {
        let mut inner = self.lock();
        let found = find_waiting(&mut inner, user, code)?;
        found.answer = if approve {
            Answer::Approved(user)
        } else {
            Answer::Declined
        };
        Ok(found.device_name.clone())
    }

    /// What the device with this secret should do next. An answer is given
    /// once, and ends the pairing.
    pub fn poll(&self, secret: &str) -> Poll {
        let mut inner = self.lock();
        let secret_hash = hash(secret);
        let Some(code) = inner
            .by_code
            .iter()
            .find(|(_, p)| p.secret_hash == secret_hash)
            .map(|(code, _)| code.clone())
        else {
            return Poll::Expired;
        };
        let answer = inner.by_code[&code].answer;
        match answer {
            Answer::Waiting => Poll::Waiting,
            Answer::Declined => {
                inner.by_code.remove(&code);
                Poll::Declined
            }
            Answer::Approved(user) => {
                let pairing = inner.by_code.remove(&code).expect("found above");
                Poll::Approved {
                    user,
                    device_name: pairing.device_name,
                }
            }
        }
    }
}

fn find_waiting<'a>(
    inner: &'a mut Inner,
    user: i64,
    code: &str,
) -> Result<&'a mut Pairing, PairError> {
    let misses = inner.misses.entry(user).or_default();
    while misses.front().is_some_and(|t| t.elapsed() >= WAIT) {
        misses.pop_front();
    }
    if misses.len() >= MAX_MISSES {
        return Err(PairError::TooManyMisses);
    }
    let code = normalise(code);
    let waiting = inner
        .by_code
        .get(&code)
        .is_some_and(|p| p.answer == Answer::Waiting);
    if !waiting {
        misses.push_back(Instant::now());
        return Err(PairError::UnknownCode);
    }
    Ok(inner.by_code.get_mut(&code).expect("checked above"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_reaches_the_device_once() {
        let pairings = Pairings::default();
        let started = pairings.start(" Living room TV ").unwrap();
        assert_eq!(started.code.len(), 9);
        assert_eq!(pairings.poll(&started.secret), Poll::Waiting);
        // Typed loosely.
        let typed = started.code.replace('-', " ").to_lowercase();
        assert_eq!(pairings.device_name(7, &typed).unwrap(), "Living room TV");
        assert_eq!(pairings.answer(7, &typed, true).unwrap(), "Living room TV");
        assert_eq!(
            pairings.poll(&started.secret),
            Poll::Approved {
                user: 7,
                device_name: "Living room TV".into()
            }
        );
        assert_eq!(pairings.poll(&started.secret), Poll::Expired);
    }

    #[test]
    fn an_answered_code_cant_be_answered_again() {
        let pairings = Pairings::default();
        let started = pairings.start("TV").unwrap();
        pairings.answer(1, &started.code, true).unwrap();
        assert_eq!(
            pairings.answer(2, &started.code, true),
            Err(PairError::UnknownCode)
        );
        assert_eq!(
            pairings.poll(&started.secret),
            Poll::Approved {
                user: 1,
                device_name: "TV".into()
            }
        );
    }

    #[test]
    fn declining_tells_the_device() {
        let pairings = Pairings::default();
        let started = pairings.start("TV").unwrap();
        pairings.answer(1, &started.code, false).unwrap();
        assert_eq!(pairings.poll(&started.secret), Poll::Declined);
        assert_eq!(pairings.poll(&started.secret), Poll::Expired);
    }

    #[test]
    fn pairings_expire() {
        let pairings = Pairings::with_wait(Duration::ZERO);
        let started = pairings.start("TV").unwrap();
        assert_eq!(pairings.poll(&started.secret), Poll::Expired);
        assert_eq!(
            pairings.answer(1, &started.code, true),
            Err(PairError::UnknownCode)
        );
    }

    #[test]
    fn a_wrong_secret_gets_nothing() {
        let pairings = Pairings::default();
        let started = pairings.start("TV").unwrap();
        pairings.answer(1, &started.code, true).unwrap();
        assert_eq!(pairings.poll("guess"), Poll::Expired);
    }

    #[test]
    fn guessing_codes_is_limited_per_user() {
        let pairings = Pairings::default();
        let started = pairings.start("TV").unwrap();
        for _ in 0..MAX_MISSES {
            assert_eq!(
                pairings.device_name(1, "AAAA-AAAA"),
                Err(PairError::UnknownCode)
            );
        }
        // Even the right code, once over the limit.
        assert_eq!(
            pairings.device_name(1, &started.code),
            Err(PairError::TooManyMisses)
        );
        assert_eq!(pairings.device_name(2, &started.code).unwrap(), "TV");
    }

    #[test]
    fn starting_is_limited() {
        let pairings = Pairings::default();
        for _ in 0..STARTS_PER_MINUTE {
            pairings.start("TV").unwrap();
        }
        assert!(matches!(
            pairings.start("TV"),
            Err(PairError::TooManyStarts)
        ));
    }

    #[test]
    fn device_names_are_checked() {
        let pairings = Pairings::default();
        assert!(matches!(pairings.start("  "), Err(PairError::BadName)));
        assert!(matches!(
            pairings.start(&"x".repeat(65)),
            Err(PairError::BadName)
        ));
    }
}
