//! Whether this machine is online. A hook's run is an agent that needs the network, so while
//! the laptop is offline the hooks wait - see `runner` - and fire once it is back.
//!
//! Online is a TCP connection to the agents' API that opens: no DNS, no route, or a wifi that
//! is up but goes nowhere all answer offline. Nothing is sent on the connection.

use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const PROBED_HOST: &str = "api.anthropic.com:443";
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a probe that got through is believed - the runner looks every couple of seconds.
const ONLINE_BELIEVED_FOR: Duration = Duration::from_secs(30);
/// How soon an offline machine is probed again, so hooks fire soon after it is back.
const OFFLINE_BELIEVED_FOR: Duration = Duration::from_secs(5);

/// Set by tests, which may run with no network and still want their hooks to fire.
static NETWORK_CHECK_TURNED_OFF: AtomicBool = AtomicBool::new(false);

/// Make every moon in this process count as online, without probing.
pub fn turn_off_network_check() {
    NETWORK_CHECK_TURNED_OFF.store(true, Ordering::Relaxed);
}

/// The last probe's answer, and when it was made.
#[derive(Default)]
pub(super) struct NetworkCheck {
    last: Option<(bool, Instant)>,
}

impl NetworkCheck {
    /// Whether the machine is online: the last probe's answer while it is fresh, a new probe
    /// otherwise.
    pub(super) fn is_machine_online(&mut self) -> bool {
        if NETWORK_CHECK_TURNED_OFF.load(Ordering::Relaxed) {
            return true;
        }
        if let Some((online, at)) = self.last {
            let believed_for = if online {
                ONLINE_BELIEVED_FOR
            } else {
                OFFLINE_BELIEVED_FOR
            };
            if at.elapsed() < believed_for {
                return online;
            }
        }
        let online = probe_internet_reachable();
        self.last = Some((online, Instant::now()));
        online
    }
}

fn probe_internet_reachable() -> bool {
    let Ok(addresses) = PROBED_HOST.to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_offline_answer_is_believed_without_probing_again() {
        let mut check = NetworkCheck {
            last: Some((false, Instant::now())),
        };
        assert!(!check.is_machine_online());
    }

    #[test]
    fn a_stale_answer_is_probed_again() {
        let stale = Instant::now() - ONLINE_BELIEVED_FOR - Duration::from_secs(1);
        let mut check = NetworkCheck {
            last: Some((true, stale)),
        };
        check.is_machine_online();
        assert!(
            check
                .last
                .is_some_and(|(_, at)| at.elapsed() < ONLINE_BELIEVED_FOR)
        );
    }
}
