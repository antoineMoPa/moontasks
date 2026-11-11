use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    WaitingForInput,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentMessage {
    pub role: AgentMessageRole,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<AgentToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentToolCall {
    pub id: String,
    pub function: AgentToolFunction,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentToolFunction {
    pub name: String,
    /// The arguments as the model wrote them: JSON, as a string.
    pub arguments: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentPrompt {
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentConversation {
    /// Minerve's own session id, which changes when `/resume` switches session.
    pub session_id: Option<String>,
    pub state: Option<AgentState>,
    pub messages: Vec<AgentMessage>,
    pub broken: Option<String>,
}
