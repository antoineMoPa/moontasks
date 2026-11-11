// The server's own state is kept apart from the wire types, which are all the window in a
// browser compiles of this module.
pub mod image_formats;
#[cfg(not(target_arch = "wasm32"))]
mod server_state;
#[cfg(not(target_arch = "wasm32"))]
mod serving;
pub mod terminal_stream;

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
pub use server_state::*;
#[cfg(not(target_arch = "wasm32"))]
pub use serving::*;

/// The port a server listens on unless told otherwise, and the one an address without a port
/// is taken to name.
pub const DEFAULT_PORT: u16 = 42000;

/// A shell that asked for a person and has not had one since: it rang its bell, or sent the
/// notification a terminal would put on the desktop - see [`crate::attention`]. Answered by
/// typing into the shell, which takes it off.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TerminalAttentionView {
    pub terminal_id: String,
    /// What the shell is called, if it has been named - `write the parser claude - 1`.
    pub name: Option<String>,
    /// What it asked for: a bare bell, or a notification with what it said. The two are
    /// told apart because a window posts a notification and only marks a bell - see
    /// [`crate::native::model::Model::take_attention`].
    pub asked: crate::attention::Asked,
    /// When it asked, in seconds since the epoch: a window posts each ask once, and this is
    /// how it tells one it has posted from a new one.
    pub at_unix: u64,
}

/// Every shell asking for a person, as the window asks for them, and the text the shells
/// copied to the clipboard since the window last asked.
#[derive(Serialize, Deserialize)]
pub struct TerminalAttentionList {
    pub terminals: Vec<TerminalAttentionView>,
    /// Each shell's last copy with OSC 52, taken as it is asked for - see
    /// [`crate::attention::Heard::Copied`]. Missing from an older server's answer.
    #[serde(default)]
    pub copies: Vec<String>,
}

#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DiffTarget {
    pub base: Option<String>,
    pub pathspec: Option<String>,
    pub comparison: Option<[String; 2]>,
}

#[derive(Serialize, Deserialize)]
pub struct SessionOpened {
    pub session_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct SessionPayload {
    pub repo_name: String,
    pub branch_name: Option<String>,
    pub commit_base: Option<String>,
    pub commits: Vec<CommitView>,
    pub history_commits: Vec<CommitView>,
    pub history_has_more: bool,
    pub local_change_summary: LocalChangeSummary,
    pub active_commit: Option<String>,
    pub repo_path: String,
    pub read_only: bool,
    pub patch_preview_line_limit: usize,
    pub available_agents: Vec<AgentOption>,
    pub selected_agent: AgentKind,
    pub full_file_path: Option<String>,
    pub hunks: Vec<HunkView>,
    pub review_comments: Vec<ReviewCommentView>,
    pub export_text: String,
}

/// One repo and how many of its files have changed, as the submodule hub lists it: the
/// reviewed repo itself, or one of its submodules. A submodule with changed files is another
/// review the user can open beside this one.
#[derive(Clone, Serialize, Deserialize)]
pub struct RepoStatusView {
    pub repo_path: String,
    /// The repo's directory name.
    pub name: String,
    pub changed_files: usize,
    /// The commits of the repo no remote branch has yet - see
    /// [`crate::git::unpushed_commit_count`].
    pub unpushed_commits: usize,
}

/// What the submodule hub shows: the reviewed repo first, then every submodule of it.
#[derive(Clone, Serialize, Deserialize)]
pub struct SubmoduleHubPayload {
    pub root: RepoStatusView,
    pub submodules: Vec<RepoStatusView>,
}

#[derive(Serialize, Deserialize)]
pub struct CommitHistoryPayload {
    pub commits: Vec<CommitView>,
    pub has_more: bool,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct LocalChangeSummary {
    pub modified: usize,
    pub added: usize,
    pub deleted: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CommitView {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HunkView {
    pub id: String,
    pub file_path: String,
    pub change_kind: FileChangeKind,
    pub header: String,
    pub staged: bool,
    pub comment: String,
    pub comment_dispatches: Vec<CommentDispatchView>,
    pub patch_preview: String,
    pub patch_line_count: usize,
    pub added_line_count: usize,
    pub removed_line_count: usize,
    pub moved_from: Option<HunkMoveHint>,
    pub moved_to: Option<HunkMoveHint>,
    pub image_diff: Option<ImageDiffView>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ImageDiffView {
    pub before_src: Option<String>,
    pub after_src: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HunkMoveHint {
    pub target_hunk_id: String,
    pub target_file_path: String,
    pub target_header: String,
    pub score: f64,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeKind {
    Added,
    Deleted,
    #[default]
    Modified,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ReviewCommentView {
    pub hunk_id: String,
    pub comment_index: usize,
    pub file_path: String,
    pub header: String,
    pub selection: String,
    pub comment: String,
    pub resolved: bool,
    pub dispatch: CommentDispatchView,
    pub jumpable: bool,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    #[default]
    None,
    Claude,
    Codex,
    OpenCode,
    Pi,
    Minerve,
}

impl AgentKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::Pi => "Pi",
            Self::Minerve => "Minerve",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AgentOption {
    pub kind: AgentKind,
    pub label: String,
    /// Whether whoever is asking has it installed.
    pub available: bool,
    /// Whether they are logged in to it, on a server that gives each person a Unix user - see
    /// `crate::agent_sessions::logins`. `None` everywhere else, where it is not asked, and for
    /// an agent they have not installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_in: Option<bool>,
    /// What they type in a shell of theirs to log in to it. Said whenever `logged_in` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_in_command: Option<String>,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum CommentDispatchStatus {
    #[default]
    Idle,
    Batched,
    Queued,
    Running,
    Canceled,
    Completed,
    Failed,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CommentDispatchView {
    pub key: String,
    pub status: CommentDispatchStatus,
    pub detail: String,
    pub agent: AgentKind,
    pub can_cancel: bool,
    pub has_log: bool,
}

#[derive(Serialize, Deserialize)]
pub struct PatchPayload {
    pub patch: String,
}

#[derive(Serialize, Deserialize)]
pub struct FileContentPayload {
    pub file_path: String,
    pub content: String,
    /// Whether this is a file outside the repo - a dependency's source or the standard
    /// library, landed on by a jump to a definition. Those are read-only: the pane offers no
    /// save on one, and a write to it is refused repo-side whatever the pane does.
    pub outside_the_repo: bool,
    /// The command that is this file's only writer, for a file that has one: the wire's
    /// broadcasts, which `moon wire post` keeps in order - see `crate::moontasks::wire`. A
    /// tab on such a file offers no save, since a save would be a second writer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_written_by: Option<String>,
    /// The file as HEAD has it, which the editor marks the new lines of the text against:
    /// empty for a file HEAD does not have, every line of which is new, and `None` for a file
    /// outside the repo, which has no history here to be new against.
    pub committed: Option<String>,
    /// The file as a `data:` URI, for a picture: a file named as an image whose bytes are not
    /// text. A tab on one draws this and not the `content`, which is empty - a picture has no
    /// text to read or to write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_src: Option<String>,
}

/// Who last touched each stretch of a file, as `git blame` has it - see
/// [`crate::git::blame_file`]. The stretches are in order, cover every line of the text they
/// were asked about, and are cut where the commit changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlamePayload {
    pub file_path: String,
    pub chunks: Vec<BlameChunk>,
}

/// One stretch of lines that a single commit - or nothing yet - last touched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlameChunk {
    /// The lines of the text the stretch covers, as indexes from zero.
    pub lines: std::ops::Range<usize>,
    pub blamed: Blamed,
    /// The file as it was just before the change the stretch is put down to: the blamed
    /// commit's parent and the path there, or HEAD for lines not committed yet. `None` where
    /// there is no before - the commit that brought the file in.
    pub before: Option<FileVersion>,
    /// The line the stretch started on in the version the change was made in, counted from
    /// one - about where to look for it in the version before.
    pub line_in_commit: usize,
}

/// A file as one commit has it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileVersion {
    pub sha: String,
    /// The path the file had in that commit, which a rename since has changed.
    pub file_path: String,
}

/// What a stretch of lines is put down to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Blamed {
    Committed(BlamedCommit),
    /// Lines no commit has: typed into the working tree, saved or not, since the last one.
    NotYetCommitted,
}

/// The commit a stretch of lines was last changed in, as much of it as the column beside the
/// lines and the note hung off it read out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlamedCommit {
    pub sha: String,
    pub author: String,
    /// The day it was authored, as `YYYY-MM-DD` in the author's own time zone - the day the
    /// author would say they wrote it.
    pub authored_on: String,
    /// The first line of the commit message.
    pub summary: String,
}

/// Which version of a file a blame is of.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum BlameOf {
    /// A text that may not be what is on disk: the tab's buffer, edits and all, so a line
    /// typed a moment ago reads as not committed rather than as whatever used to be on that
    /// line.
    Text(String),
    /// The file as one commit has it - a blame with no uncommitted lines in it.
    Revision(String),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OpenSessionRequest {
    pub repo_path: String,
    pub diff_target: Option<DiffTarget>,
    pub active_commit: Option<String>,
}

/// A pass key the server made for a window already let in - see `POST /api/pass-key`. Only a
/// native window asks: a browser's has no Tools menu to hand one on from.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize, Deserialize)]
pub struct PassKeyMinted {
    pub pass_key: String,
}

/// What `POST /api/login-ticket` is asked: how long the ticket is to be good for.
#[derive(Serialize, Deserialize)]
pub struct LoginTicketRequest {
    pub lifetime_seconds: u64,
}

/// A login ticket the server made for a window already let in, to open a browser with - see
/// `POST /api/login-ticket`.
#[derive(Serialize, Deserialize)]
pub struct LoginTicketMinted {
    pub ticket: String,
}

#[derive(Serialize, Deserialize)]
pub struct CommitRunStarted {
    pub terminal_id: String,
}

/// One shell as the server has it: which one, and what it is called.
#[derive(Serialize, Deserialize)]
pub struct TerminalView {
    pub terminal_id: String,
    /// What its tab reads, if it has been named - see `TerminalRegistry::name`.
    pub name: Option<String>,
}

/// A shell being renamed: what it is to be called.
#[derive(Serialize, Deserialize)]
pub struct TerminalNameRequest {
    pub name: String,
}

/// How a commit run ended. `None` while it is still going.
#[derive(Serialize, Deserialize)]
pub struct CommitRunOutcome {
    pub exit_code: Option<i32>,
}

/// The types a language question and its answer are made of, from the client crate.
///
/// Re-exported rather than redefined so that the wire format and the client's own types are
/// one thing: a `--remote` window serialises exactly what [`moon_lsp`] hands back.
pub use moon_lsp::{
    LspCompletion, LspFileEdit, LspLocation, LspPlaces, LspPosition, LspStatus, LspWork,
};

/// A question about the places of one kind for the name at one place in one file.
#[derive(Serialize, Deserialize)]
pub struct LspPlacesRequest {
    pub file_path: String,
    pub at: LspPosition,
    pub which: LspPlaces,
}

/// A file to format, and how the repo indents.
#[derive(Serialize, Deserialize)]
pub struct LspFormatRequest {
    pub file_path: String,
    pub options: moon_lsp::LspFormatting,
}

/// The edits that format one file.
#[derive(Serialize, Deserialize)]
pub struct LspTextEditsPayload {
    pub edits: Vec<moon_lsp::LspTextEdit>,
}

/// What a server says about a name, as markdown.
#[derive(Serialize, Deserialize)]
pub struct LspHoverPayload {
    pub markdown: Option<String>,
}

/// What a server last said is wrong with a file.
#[derive(Serialize, Deserialize)]
pub struct LspDiagnosticsPayload {
    pub diagnostics: Vec<moon_lsp::LspDiagnostic>,
}

/// What a server offers to do at a place.
#[derive(Serialize, Deserialize)]
pub struct LspCodeActionsPayload {
    pub actions: Vec<moon_lsp::LspCodeAction>,
}

/// The signature of the call around a place.
#[derive(Serialize, Deserialize)]
pub struct LspSignaturePayload {
    pub signature: Option<moon_lsp::LspSignature>,
}

/// A new name for the name at one place in one file.
#[derive(Serialize, Deserialize)]
pub struct LspRenameRequest {
    pub file_path: String,
    pub at: LspPosition,
    pub new_name: String,
}

/// What the name at a place is called, as the server would rename it. `None` is nothing there
/// that can be renamed.
#[derive(Serialize, Deserialize)]
pub struct LspRenamablePayload {
    pub name: Option<String>,
}

/// Everything a rename changes, one entry per file.
#[derive(Serialize, Deserialize)]
pub struct LspFileEditsPayload {
    pub files: Vec<LspFileEdit>,
}

/// A file the editor has opened or changed, and what is in it now.
#[derive(Serialize, Deserialize)]
pub struct LspDocumentRequest {
    pub file_path: String,
    pub text: String,
}

/// A question about one place in one file.
#[derive(Serialize, Deserialize)]
pub struct LspPositionRequest {
    pub file_path: String,
    pub at: LspPosition,
}

#[derive(Serialize, Deserialize)]
pub struct LspStatusPayload {
    pub status: LspStatus,
}

#[derive(Serialize, Deserialize)]
pub struct LspWorkPayload {
    pub working: Vec<LspWork>,
}

#[derive(Serialize, Deserialize)]
pub struct LspLocationsPayload {
    pub locations: Vec<LspLocation>,
}

#[derive(Serialize, Deserialize)]
pub struct LspCompletionsPayload {
    pub completions: Vec<LspCompletion>,
}

/// The characters a language server said should open a completion list on their own - the
/// `.` of `thing.`, the `:` of a path. One answer per file, and it does not change for as
/// long as that server runs.
#[derive(Serialize, Deserialize)]
pub struct LspTriggersPayload {
    pub triggers: Vec<char>,
}

/// Which files a search reads: the files of the repo, or those and the ones its `.gitignore`
/// leaves out - a submodule some repos ignore, a vendored directory.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SearchScope {
    #[default]
    RepoFiles,
    IncludingIgnored,
}

impl SearchScope {
    /// The scope a checkbox reading "include gitignored" stands for, ticked or not.
    pub fn including_ignored(included: bool) -> Self {
        if included {
            Self::IncludingIgnored
        } else {
            Self::RepoFiles
        }
    }

    pub fn includes_ignored(self) -> bool {
        self == Self::IncludingIgnored
    }
}

/// What a search has found so far. A search reports one of these every time what it has
/// found changes, and once more, marked done, when it is over; each stands on its own - the
/// matches are the whole of what is worth showing, not the ones since the report before.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchProgress<T> {
    pub matches: Vec<T>,
    /// Set when there were more matches than the search hands back, so the palette can say
    /// that narrowing the query would show different rows rather than only fewer.
    pub truncated: bool,
    pub done: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> SearchProgress<T> {
    /// A search that is over without having had anything to look for.
    pub fn nothing() -> Self {
        Self {
            matches: Vec::new(),
            truncated: false,
            done: true,
        }
    }
}

/// One line of a search streamed over the wire: a report, or the reason the search failed,
/// which comes last when it comes at all - the response is under way by then, so a status
/// line cannot carry it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchLine<T> {
    Found(SearchProgress<T>),
    Failed(String),
}

/// One line of the repo that a content search found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentMatch {
    pub file_path: String,
    /// Counted from one, the way the number in an editor's fringe is.
    pub line_number: usize,
    /// The line itself, trimmed of its indentation and cut short if it was a long one.
    pub line: String,
}

/// How many commits of the history one page of it holds: the first page comes with the
/// session, and the window asks for each next one by this size too.
pub const HISTORY_COMMIT_PAGE_SIZE: usize = 30;

#[derive(Deserialize, Clone)]
pub struct CommentRequest {
    pub hunk_id: String,
    pub comment: String,
    #[serde(default)]
    pub batch: bool,
    #[serde(default)]
    pub patch_excerpt: String,
}

/// An agent already running on a task, which a comment can go to instead of starting a run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunningAgent {
    pub terminal_id: String,
    /// What the run's shell is called, which is what the review shows it as.
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct AgentLogPayload {
    pub dispatch_key: String,
    pub text: String,
}

pub mod profiles;
