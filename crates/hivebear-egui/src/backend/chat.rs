//! Conversation history. Synchronous SQLite calls — run them through
//! `Ctx::blocking`, not on the UI thread.

use crate::error::CmdResult;
use crate::state::AppState;
use crate::validation;
use hivebear_persistence::{Conversation, MessageRole, PersistedMessage};

pub fn list_conversations(state: &AppState) -> CmdResult<Vec<Conversation>> {
    state
        .chat_db
        .list_conversations()
        .map_err(|e| e.to_string())
}

pub fn create_conversation(
    state: &AppState,
    title: &str,
    model_id: &str,
) -> CmdResult<Conversation> {
    validation::validate_title(title)?;
    validation::validate_title(model_id)?;
    state
        .chat_db
        .create_conversation(title, model_id)
        .map_err(|e| e.to_string())
}

pub fn get_conversation_messages(
    state: &AppState,
    conversation_id: &str,
) -> CmdResult<Vec<PersistedMessage>> {
    validation::validate_conversation_id(conversation_id)?;
    state
        .chat_db
        .get_messages(conversation_id)
        .map_err(|e| e.to_string())
}

pub fn add_message(
    state: &AppState,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> CmdResult<PersistedMessage> {
    validation::validate_conversation_id(conversation_id)?;
    validation::validate_role(role)?;
    validation::validate_message_content(content)?;
    let msg_role = MessageRole::parse(role);
    state
        .chat_db
        .add_message(conversation_id, msg_role, content, None)
        .map_err(|e| e.to_string())
}

pub fn delete_conversation(state: &AppState, conversation_id: &str) -> CmdResult<()> {
    validation::validate_conversation_id(conversation_id)?;
    state
        .chat_db
        .delete_conversation(conversation_id)
        .map_err(|e| e.to_string())
}

pub fn rename_conversation(
    state: &AppState,
    conversation_id: &str,
    new_title: &str,
) -> CmdResult<()> {
    validation::validate_conversation_id(conversation_id)?;
    validation::validate_title(new_title)?;
    state
        .chat_db
        .rename_conversation(conversation_id, new_title)
        .map_err(|e| e.to_string())
}
