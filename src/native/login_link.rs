//! Login link - builds a link that logs a browser in: the page `moon serve` serves at `/moon`,
//! on the repo and the frame a window is on, with a login ticket in its fragment. See
//! `src/server/login.html`, which redeems the ticket and takes it off the address.
//!

use std::path::Path;

use crate::{backend::remote::urlencode, cli::Frame};

/// The link: `server`'s page on `repo` and `frame`, which `ticket` logs into. `server` is an
/// origin - `https://moon.example.com` - with no slash after it.
pub(crate) fn address(server: &str, repo: &Path, frame: Frame, ticket: &str) -> String {
    format!(
        "{server}/moon/?repo={}&frame={}#ticket={ticket}",
        urlencode(&repo.to_string_lossy()),
        urlencode(frame.subcommand()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo and the frame are spelled so they survive an address, and the ticket rides in
    /// the fragment, which a browser never sends to a server.
    #[test]
    fn the_link_names_the_repo_and_frame_and_carries_the_ticket_in_its_fragment() {
        assert_eq!(
            address(
                "https://moon.example.com",
                Path::new("/home/dev/my repo"),
                Frame::Tasks,
                "t.payload.mac",
            ),
            format!(
                "https://moon.example.com/moon/?repo=%2Fhome%2Fdev%2Fmy%20repo&frame={}#ticket=t.payload.mac",
                Frame::Tasks.subcommand()
            )
        );
    }
}
