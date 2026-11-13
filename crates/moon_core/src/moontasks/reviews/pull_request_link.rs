//! Where a review's `[pr]` button goes: the pull request's page on the forge its repo is on.
//!
//! Worked out from the repo's `origin` - GitHub and GitLab, by their own hosts - so a board on
//! either needs nothing set. Any other forge is named in the board's `.moontasks.json`:
//!
//! ```json
//! { "pull_request_url": "https://git.example.com/{repo}/pulls/{pr}" }
//! ```
//!
//! `{repo}` is the remote's path - `owner/name` - and `{pr}` the number the file gives. A `pr:`
//! that is a link already is used as it is.

use std::path::Path;

/// The board's own setting for forges that are not GitHub or GitLab.
const PULL_REQUEST_URL_KEY: &str = "pull_request_url";

/// The pull request's page, or `None` when it cannot be told where that is.
pub fn find_pull_request_url(board_repo: &Path, working_copy: &Path, pr: &str) -> Option<String> {
    if pr.starts_with("http://") || pr.starts_with("https://") {
        return Some(pr.to_string());
    }
    let remote = crate::git::run_git(working_copy, &["remote", "get-url", "origin"]).ok()?;
    let (host, repo) = split_remote(remote.trim())?;
    if let Some(template) = read_pull_request_template(board_repo) {
        return Some(template.replace("{repo}", &repo).replace("{pr}", pr));
    }
    match host.as_str() {
        "github.com" => Some(format!("https://github.com/{repo}/pull/{pr}")),
        host if host.contains("gitlab") => Some(format!("https://{host}/{repo}/-/merge_requests/{pr}")),
        _ => None,
    }
}

/// `git@github.com:owner/name.git` and `https://github.com/owner/name` alike, as the host and
/// `owner/name`.
fn split_remote(remote: &str) -> Option<(String, String)> {
    let (host, path) = match remote.split_once("://") {
        // `https://host/owner/name`, `ssh://git@host:22/owner/name`: the host ends at the first `/`.
        Some((_, rest)) => rest.split_once('/')?,
        // `git@host:owner/name`, the scp form: at the `:`.
        None => remote.split_once(':')?,
    };
    let host = host.rsplit('@').next()?.split(':').next()?.to_string();
    let repo = path.trim_matches('/').trim_end_matches(".git").to_string();
    (!host.is_empty() && !repo.is_empty()).then_some((host, repo))
}

fn read_pull_request_template(board_repo: &Path) -> Option<String> {
    let text = std::fs::read_to_string(board_repo.join(".moontasks.json")).ok()?;
    let config: serde_json::Value = serde_json::from_str(&text).ok()?;
    config
        .get(PULL_REQUEST_URL_KEY)?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::split_remote;

    #[test]
    fn a_remote_is_split_into_its_host_and_its_repo_in_either_form() {
        assert_eq!(
            split_remote("git@github.com:antoineMoPa/moon-dev-tools.git"),
            Some(("github.com".to_string(), "antoineMoPa/moon-dev-tools".to_string()))
        );
        assert_eq!(
            split_remote("https://gitlab.example.com/group/sub/name.git"),
            Some(("gitlab.example.com".to_string(), "group/sub/name".to_string()))
        );
        assert_eq!(
            split_remote("ssh://git@github.com:22/o/r.git"),
            Some(("github.com".to_string(), "o/r".to_string()))
        );
    }
}
