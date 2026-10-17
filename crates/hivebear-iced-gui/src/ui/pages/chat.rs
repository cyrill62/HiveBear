use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use iced::futures::{channel::mpsc, stream, StreamExt};
use iced::keyboard::{key::Named, Key};
use iced::widget::{
    button, column, container, hover, markdown, pick_list, row, rule, scrollable, slider, space,
    text_editor, text_input,
};
use iced::{Alignment, Element, Fill, Length, Task};

use hivebear_inference::types::ChatMessage;
use hivebear_persistence::{Conversation, MessageRole, PersistedMessage};

use crate::backend::{self, inference::ChatParams, inference::StreamEnd};
use crate::error::CmdResult;
use crate::state::AppState;
use crate::ui::icon::{self, icon};
use crate::ui::theme::{self, *};
use crate::ui::util::model_display_name;
use crate::ui::widgets::{self, Kind, Pad};
use crate::ui::{blocking, Cx, Env, Route};

const CONTENT_WIDTH: f32 = 672.0;

const SUGGESTIONS: &[(&str, &str)] = &[
    ("Explain quantum computing", "Break it down in simple terms"),
    (
        "Write a Python sort function",
        "With type hints and docstring",
    ),
    ("Compare Rust vs Go", "Pros, cons, and use cases"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    System,
    User,
    Assistant,
}

struct ChatMsg {
    role: Role,
    content: String,
    /// Parsed incrementally as tokens arrive.
    markdown: markdown::Content,
    /// An error shown in place of a reply. Not sent back to the model.
    error: bool,
}

impl ChatMsg {
    fn new(role: Role, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            role,
            markdown: markdown::Content::parse(&content),
            content,
            error: false,
        }
    }

    fn set_content(&mut self, content: String) {
        self.markdown = markdown::Content::parse(&content);
        self.content = content;
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ConversationCreated(Conversation),
    Done(CmdResult<StreamEnd>),
}

/// A loaded model as the selector shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelChoice {
    handle_id: u64,
    label: String,
}

impl std::fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Input(text_editor::Action),
    Send,
    Suggest(String),
    Stop,
    Stream(StreamEvent),
    ToggleParams,
    Temperature(f32),
    TopP(f32),
    MaxTokens(String),
    ResetParams,
    SelectModel(ModelChoice),
    LoadModel(String),

    ToggleDrawer,
    NewChat,
    DrawerSearch(String),
    SelectConversation(String),
    StartRename(String),
    RenameInput(String),
    CommitRename,
    Delete(String),
    ConversationsLoaded(CmdResult<Vec<Conversation>>),
    MessagesLoaded(CmdResult<Vec<PersistedMessage>>),
    ConversationChanged(CmdResult<()>),

    Edit(usize),
    EditInput(text_editor::Action),
    SubmitEdit,
    CancelEdit,
    Regenerate,
    Copy(usize),
    OpenLink(String),

    Go(Route),
    DismissNudge,
}

struct Streaming {
    cancel: Arc<AtomicBool>,
    started: Instant,
}

#[derive(Default)]
pub struct Chat {
    messages: Vec<ChatMsg>,
    input: text_editor::Content,
    params: ChatParams,
    max_tokens_text: String,
    params_open: bool,

    drawer_open: bool,
    drawer_search: String,
    renaming: Option<(String, String)>,
    conversations: Vec<Conversation>,
    convs_loaded: bool,
    active_conv: Option<String>,

    editing: Option<(usize, text_editor::Content)>,
    streaming: Option<Streaming>,
    show_nudge: bool,
    new_chat_requested: bool,
}

impl Chat {
    pub fn enter(&mut self, env: &mut Env) -> Task<Msg> {
        env.shared.refresh_loaded(env.state);
        let t = env.shared.refresh_installed(env.state);
        env.spawn(t);
        let mut tasks = vec![self.flush_new_chat(env)];
        if !self.convs_loaded {
            tasks.push(self.refresh_conversations(env.state));
        }
        Task::batch(tasks)
    }

    /// Ctrl/⌘+N or the palette: start a fresh conversation when the page opens.
    pub fn request_new_chat(&mut self) {
        self.new_chat_requested = true;
    }

    pub fn flush_new_chat(&mut self, _env: &mut Env) -> Task<Msg> {
        if std::mem::take(&mut self.new_chat_requested) {
            self.new_chat();
        }
        Task::none()
    }

    /// Whether the "thinking" dots are on screen and need animation frames.
    pub fn animating(&self) -> bool {
        self.streaming.is_some() && self.messages.last().is_some_and(|m| m.content.is_empty())
    }

    fn refresh_conversations(&self, state: &Arc<AppState>) -> Task<Msg> {
        blocking(
            state,
            backend::chat::list_conversations,
            Msg::ConversationsLoaded,
        )
    }

    fn new_chat(&mut self) {
        if let Some(s) = &self.streaming {
            s.cancel.store(true, Ordering::Relaxed);
        }
        self.active_conv = None;
        self.messages.clear();
        self.editing = None;
        self.drawer_open = false;
    }

    /// Send `text` as the next user turn.
    ///
    /// `persist_user` is false for regenerate and edit-and-resend, which
    /// record only the new reply.
    fn send(&mut self, env: &mut Env, text: String, persist_user: bool) -> Task<Msg> {
        let Some(handle_id) = env.shared.active_handle else {
            return Task::none();
        };
        if self.streaming.is_some() || text.trim().is_empty() {
            return Task::none();
        }

        let mut history: Vec<ChatMessage> = self
            .messages
            .iter()
            .filter(|m| !m.error)
            .map(|m| match m.role {
                Role::System => ChatMessage::System(m.content.clone()),
                Role::Assistant => ChatMessage::assistant(m.content.clone()),
                Role::User => ChatMessage::user_text(&m.content),
            })
            .collect();
        history.push(ChatMessage::user_text(&text));
        self.messages.push(ChatMsg::new(Role::User, text.clone()));
        self.messages.push(ChatMsg::new(Role::Assistant, ""));

        let create = persist_user && self.active_conv.is_none();
        let title = if text.chars().count() > 40 {
            format!("{}...", text.chars().take(40).collect::<String>())
        } else {
            text.clone()
        };
        let model_name = env
            .shared
            .loaded
            .iter()
            .find(|m| m.handle_id == handle_id)
            .map_or_else(|| "chat".to_string(), |m| model_display_name(&m.model_path));

        let cancel = Arc::new(AtomicBool::new(false));
        self.streaming = Some(Streaming {
            cancel: Arc::clone(&cancel),
            started: Instant::now(),
        });

        let state = Arc::clone(env.state);
        let params = self.params;
        let mut conv_id = self.active_conv.clone();
        // Unbounded: a token must never be dropped because the UI is busy.
        let (tx, rx) = mpsc::unbounded();
        let work = async move {
            if create {
                let (t, m) = (title, model_name);
                match run_blocking(&state, move |s| {
                    backend::chat::create_conversation(s, &t, &m)
                })
                .await
                {
                    Ok(conv) => {
                        conv_id = Some(conv.id.clone());
                        let _ = tx.unbounded_send(StreamEvent::ConversationCreated(conv));
                    }
                    Err(e) => tracing::warn!("Failed to create conversation: {e}"),
                }
            }
            if let (true, Some(id)) = (persist_user, conv_id.clone()) {
                if let Err(e) = run_blocking(&state, move |s| {
                    backend::chat::add_message(s, &id, "user", &text)
                })
                .await
                {
                    tracing::warn!("Failed to save message: {e}");
                }
            }

            let token_tx = tx.clone();
            let end =
                backend::inference::stream_chat(&state, handle_id, history, params, &cancel, |t| {
                    let _ = token_tx.unbounded_send(StreamEvent::Token(t.to_string()));
                })
                .await;

            if let (Ok(StreamEnd::Done(reply)), Some(id)) = (&end, conv_id) {
                if !reply.is_empty() {
                    let reply = reply.clone();
                    if let Err(e) = run_blocking(&state, move |s| {
                        backend::chat::add_message(s, &id, "assistant", &reply)
                    })
                    .await
                    {
                        tracing::warn!("Failed to save reply: {e}");
                    }
                }
            }
            let _ = tx.unbounded_send(StreamEvent::Done(end));
        };
        let events = stream::select(rx, stream::once(work).filter_map(|()| async { None }));
        Task::run(events, Msg::Stream)
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            Msg::Input(action) => self.input.perform(action),
            Msg::Send => {
                let text = self.input.text().trim().to_string();
                if self.streaming.is_none() && !text.is_empty() {
                    self.input = text_editor::Content::new();
                    return self.send(env, text, true);
                }
            }
            Msg::Suggest(text) => return self.send(env, text, true),
            Msg::Stop => {
                if let Some(s) = &self.streaming {
                    s.cancel.store(true, Ordering::Relaxed);
                }
            }
            Msg::Stream(event) => return self.on_stream(event, env),

            Msg::ToggleParams => {
                self.params_open = !self.params_open;
                self.max_tokens_text = self.params.max_tokens.to_string();
            }
            Msg::Temperature(v) => self.params.temperature = (v * 10.0).round() / 10.0,
            Msg::TopP(v) => self.params.top_p = (v * 20.0).round() / 20.0,
            Msg::MaxTokens(s) => {
                if let Ok(n) = s.parse::<u32>() {
                    self.params.max_tokens = n.clamp(128, 131_072);
                }
                self.max_tokens_text = s;
            }
            Msg::ResetParams => {
                self.params = ChatParams::default();
                self.max_tokens_text = self.params.max_tokens.to_string();
            }
            Msg::SelectModel(choice) => env.shared.active_handle = Some(choice.handle_id),
            Msg::LoadModel(id) => {
                let t = env.shared.load_model(env.state, &id, false);
                env.spawn(t);
            }

            Msg::ToggleDrawer => self.drawer_open = !self.drawer_open,
            Msg::NewChat => self.new_chat(),
            Msg::DrawerSearch(q) => self.drawer_search = q,
            Msg::SelectConversation(id) if self.streaming.is_none() => {
                self.active_conv = Some(id.clone());
                self.editing = None;
                self.drawer_open = false;
                return blocking(
                    env.state,
                    move |s| backend::chat::get_conversation_messages(s, &id),
                    Msg::MessagesLoaded,
                );
            }
            Msg::SelectConversation(_) => env.toasts.error("Wait for the reply to finish."),
            Msg::StartRename(id) => {
                let title = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.title.clone())
                    .unwrap_or_default();
                self.renaming = Some((id, title));
            }
            Msg::RenameInput(t) => {
                if let Some((_, text)) = &mut self.renaming {
                    *text = t;
                }
            }
            Msg::CommitRename => {
                if let Some((id, title)) = self.renaming.take() {
                    let title = title.trim().to_string();
                    if !title.is_empty() {
                        return blocking(
                            env.state,
                            move |s| backend::chat::rename_conversation(s, &id, &title),
                            Msg::ConversationChanged,
                        );
                    }
                }
            }
            Msg::Delete(id) => {
                if self.active_conv.as_deref() == Some(id.as_str()) {
                    self.active_conv = None;
                    self.messages.clear();
                }
                return blocking(
                    env.state,
                    move |s| backend::chat::delete_conversation(s, &id),
                    Msg::ConversationChanged,
                );
            }
            Msg::ConversationsLoaded(result) => {
                self.convs_loaded = true;
                match result {
                    Ok(c) => self.conversations = c,
                    Err(e) => env
                        .toasts
                        .error(format!("Couldn't load conversations: {e}")),
                }
            }
            Msg::MessagesLoaded(result) => match result {
                Ok(persisted) => {
                    self.messages = persisted
                        .into_iter()
                        .map(|m| {
                            let role = match m.role {
                                MessageRole::User => Role::User,
                                MessageRole::Assistant => Role::Assistant,
                                MessageRole::System => Role::System,
                            };
                            ChatMsg::new(role, m.content)
                        })
                        .collect();
                }
                Err(e) => env.toasts.error(format!("Couldn't load messages: {e}")),
            },
            Msg::ConversationChanged(result) => {
                if let Err(e) = result {
                    env.toasts.error(e);
                }
                return self.refresh_conversations(env.state);
            }

            Msg::Edit(i) => {
                if let Some(m) = self.messages.get(i) {
                    self.editing = Some((i, text_editor::Content::with_text(&m.content)));
                }
            }
            Msg::EditInput(action) => {
                if let Some((_, content)) = &mut self.editing {
                    content.perform(action);
                }
            }
            Msg::CancelEdit => self.editing = None,
            Msg::SubmitEdit => {
                if let Some((i, content)) = self.editing.take() {
                    self.messages.truncate(i);
                    return self.send(env, content.text().trim().to_string(), false);
                }
            }
            Msg::Regenerate => {
                let n = self.messages.len();
                if n >= 2
                    && self.messages[n - 1].role == Role::Assistant
                    && self.messages[n - 2].role == Role::User
                {
                    self.messages.pop();
                    if let Some(user) = self.messages.pop() {
                        return self.send(env, user.content, false);
                    }
                }
            }
            Msg::Copy(i) => {
                if let Some(m) = self.messages.get(i) {
                    env.copy(m.content.clone());
                }
            }
            Msg::OpenLink(url) => crate::ui::util::open_url(&url),

            Msg::Go(route) => {
                // Shown once: gone for good once clicked or dismissed.
                env.prefs.first_chat_nudge_dismissed = true;
                self.show_nudge = false;
                env.go(route);
            }
            Msg::DismissNudge => {
                env.prefs.first_chat_nudge_dismissed = true;
                self.show_nudge = false;
            }
        }
        Task::none()
    }

    fn on_stream(&mut self, event: StreamEvent, env: &mut Env) -> Task<Msg> {
        let stopped = self
            .streaming
            .as_ref()
            .is_some_and(|s| s.cancel.load(Ordering::Relaxed));
        match event {
            StreamEvent::Token(t) => {
                if !stopped {
                    if let Some(last) = self.messages.last_mut() {
                        last.content.push_str(&t);
                        last.markdown.push_str(&t);
                    }
                }
            }
            StreamEvent::ConversationCreated(conv) => {
                self.active_conv = Some(conv.id.clone());
                self.conversations.insert(0, conv);
            }
            StreamEvent::Done(result) => {
                self.streaming = None;
                let failure = match result {
                    Ok(StreamEnd::Done(reply)) => {
                        if let Some(last) = self.messages.last_mut() {
                            if last.content != reply {
                                last.set_content(reply.clone());
                            }
                        }
                        if !reply.is_empty() && !env.prefs.first_chat_nudge_dismissed {
                            self.show_nudge = true;
                        }
                        None
                    }
                    Ok(StreamEnd::Failed(e)) | Err(e) => Some(e),
                };
                if let (Some(e), Some(last)) = (failure, self.messages.last_mut()) {
                    last.set_content(format!("Error: {e}"));
                    last.error = true;
                }
                return self.refresh_conversations(env.state);
            }
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let title = self
            .active_conv
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .map_or(
                if self.active_conv.is_some() {
                    "Chat"
                } else {
                    "New Chat"
                },
                |c| c.title.as_str(),
            );
        let top = container(
            row![
                widgets::icon_button(icon::SIDEBAR_SIMPLE, "Conversations", Msg::ToggleDrawer),
                widgets::strong(title),
                space::horizontal(),
                widgets::icon_button(icon::PLUS, "New chat", Msg::NewChat),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 16]);

        let has_loaded = !cx.shared.loaded.is_empty();
        let body: Element<'a, Msg> = if !has_loaded {
            self.no_model(cx)
        } else if self.messages.is_empty() {
            self.empty_conversation(cx)
        } else {
            self.message_list(cx)
        };

        let mut main = column![top, rule::horizontal(1), container(body).height(Fill)];
        if has_loaded {
            main = main.push(rule::horizontal(1)).push(self.input_area(cx));
        }

        if self.drawer_open {
            row![self.drawer(), rule::vertical(1), main].into()
        } else {
            main.into()
        }
    }

    fn no_model<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let shared = cx.shared;
        let mut actions = column![].spacing(8).align_x(Alignment::Center);
        if shared.installed.is_empty() {
            actions = actions.push(widgets::muted(
                "Install a model from the Model Browser first.",
            ));
        } else {
            let mut buttons = row![].spacing(8);
            for m in shared.installed.iter().take(4) {
                let busy = shared.loading_model.as_deref() == Some(m.id.as_str());
                buttons = buttons.push(widgets::btn_sm(
                    Kind::Secondary,
                    busy.then_some(icon::ARROWS_CLOCKWISE),
                    m.name.as_str(),
                    shared
                        .loading_model
                        .is_none()
                        .then(|| Msg::LoadModel(m.id.clone())),
                ));
            }
            actions = actions.push(buttons);
        }
        if let Some(e) = &shared.load_error {
            actions = actions.push(widgets::label(e.as_str(), SMALL, DANGER));
        }
        container(widgets::empty_state(
            icon::SPARKLE,
            "No model loaded",
            "Load an installed model to start chatting.",
            Some(actions.into()),
        ))
        .center(Fill)
        .into()
    }

    fn empty_conversation<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let heading = active_model_name(cx).map_or_else(
            || "What can I help you with?".to_string(),
            |m| format!("Chat with {m}"),
        );
        let mut col = column![
            widgets::icon_tile(icon::SPARKLE, 64.0, tint(PAW_500, 0.1), PAW_500),
            widgets::strong(heading),
            widgets::muted("Try a suggestion or type your own message."),
            space().height(8),
        ]
        .spacing(6)
        .align_x(Alignment::Center)
        .max_width(448);
        for (title, desc) in SUGGESTIONS {
            col = col.push(widgets::clickable_card(
                Pad::Sm,
                column![
                    widgets::label(*title, SMALL, TEXT_PRIMARY).font(widgets::BOLD),
                    widgets::label(*desc, TINY, TEXT_MUTED),
                ],
                Msg::Suggest(format!("{title} — {desc}")),
            ));
        }
        container(col).center(Fill).into()
    }

    fn message_list<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let model = active_model_name(cx).unwrap_or_else(|| "Assistant".into());
        let streaming = self.streaming.is_some();
        let last = self.messages.len().saturating_sub(1);
        let md_settings = markdown::Settings::with_text_size(
            BODY,
            markdown::Style::from_palette(theme::app_theme().palette()),
        );

        let mut list = column![].spacing(20).max_width(CONTENT_WIDTH);
        for (i, msg) in self.messages.iter().enumerate() {
            if let Some((idx, content)) = &self.editing {
                if *idx == i && msg.role == Role::User {
                    list = list.push(edit_box(content));
                    continue;
                }
            }
            let is_streaming = streaming && i == last && msg.role == Role::Assistant;
            let who = match msg.role {
                Role::User => "You".to_string(),
                Role::System => "System".to_string(),
                Role::Assistant => model.clone(),
            };

            let body: Element<'a, Msg> = if msg.error {
                widgets::label(msg.content.as_str(), BODY, DANGER).into()
            } else if msg.role == Role::User {
                widgets::primary(msg.content.as_str()).into()
            } else if msg.content.is_empty() && is_streaming {
                thinking_dots(
                    self.streaming
                        .as_ref()
                        .map_or_else(Instant::now, |s| s.started),
                )
            } else {
                let rendered = markdown::view(msg.markdown.items(), md_settings).map(Msg::OpenLink);
                if is_streaming {
                    column![rendered, widgets::label("▍", BODY, PAW_500)].into()
                } else {
                    rendered
                }
            };

            let mut actions = row![].spacing(0);
            if !msg.content.is_empty() && !is_streaming {
                actions = actions.push(widgets::icon_button(icon::COPY, "Copy", Msg::Copy(i)));
                if msg.role == Role::User && !streaming {
                    actions = actions.push(widgets::icon_button(
                        icon::PENCIL_SIMPLE,
                        "Edit and resend",
                        Msg::Edit(i),
                    ));
                }
                if msg.role == Role::Assistant && i == last && !streaming {
                    actions = actions.push(widgets::icon_button(
                        icon::ARROWS_CLOCKWISE,
                        "Regenerate",
                        Msg::Regenerate,
                    ));
                }
            }
            let header = |actions: Element<'a, Msg>| -> Element<'a, Msg> {
                row![
                    widgets::label(who.clone(), TINY, TEXT_MUTED).font(widgets::BOLD),
                    space::horizontal(),
                    actions
                ]
                .align_y(Alignment::Center)
                .height(28)
                .into()
            };
            // Actions appear on hover, as in the web app.
            let content = hover(
                column![header(space().into()), body].spacing(4),
                header(actions.into()),
            );
            list = list.push(row![avatar(msg.role), content].spacing(12));
        }

        scrollable(container(list).padding(24).center_x(Fill))
            .anchor_bottom()
            .height(Fill)
            .into()
    }

    fn input_area<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let shared = cx.shared;
        let enabled = shared.active_handle.is_some();
        let streaming = self.streaming.is_some();

        let mut col = column![].spacing(8).max_width(CONTENT_WIDTH);

        if self.show_nudge && !streaming {
            col = col.push(
                container(
                    row![
                        icon(icon::GAUGE).color(PAW_500),
                        widgets::link(
                            "See how fast your machine is — run a 30s benchmark →",
                            TEXT_PRIMARY,
                            Msg::Go(Route::Benchmark)
                        ),
                        space::horizontal(),
                        widgets::link(
                            format!("{}  Join the hive", icon::NETWORK),
                            TEXT_MUTED,
                            Msg::Go(Route::Mesh)
                        ),
                        widgets::icon_button(icon::X, "Dismiss", Msg::DismissNudge),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([6, 14])
                .style(theme::accent_card),
            );
        }

        // Model selector + parameters
        let selector: Element<'a, Msg> = if shared.loaded.len() > 1 {
            let choices: Vec<ModelChoice> = shared
                .loaded
                .iter()
                .map(|m| ModelChoice {
                    handle_id: m.handle_id,
                    label: format!("{} ({})", model_display_name(&m.model_path), m.engine),
                })
                .collect();
            let selected = choices
                .iter()
                .find(|c| Some(c.handle_id) == shared.active_handle)
                .cloned();
            pick_list(choices, selected, Msg::SelectModel)
                .text_size(TINY)
                .padding([3, 8])
                .into()
        } else if let Some(name) = active_model_name(cx) {
            container(
                row![
                    widgets::dot(SUCCESS, 3.0),
                    widgets::label(name, TINY, TEXT_SECONDARY)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([3, 8])
            .style(theme::pill(SURFACE_RAISED, RADIUS_SM))
            .into()
        } else {
            space().into()
        };
        let params_color = if self.params_open || self.params != ChatParams::default() {
            PAW_400
        } else {
            TEXT_MUTED
        };
        col = col.push(
            row![
                selector,
                space::horizontal(),
                button(icon(icon::SLIDERS).color(params_color))
                    .padding(6)
                    .style(theme::button(Kind::Ghost))
                    .on_press(Msg::ToggleParams),
            ]
            .align_y(Alignment::Center),
        );

        if self.params_open {
            col = col.push(self.params_panel());
        }

        // Enter sends; Shift+Enter is a new line.
        let editor = text_editor(&self.input)
            .placeholder("Message HiveBear...")
            .height(Length::Shrink)
            .min_height(44)
            .max_height(200)
            .padding([10, 12])
            .size(BODY)
            .style(theme::editor)
            .key_binding(|kp| match kp.key.as_ref() {
                Key::Named(Named::Enter) if !kp.modifiers.shift() => {
                    Some(text_editor::Binding::Custom(Msg::Send))
                }
                _ => text_editor::Binding::from_key_press(kp),
            });
        let editor = if enabled {
            editor.on_action(Msg::Input)
        } else {
            editor
        };

        let action = if streaming {
            button(icon(icon::STOP).size(14))
                .padding(9)
                .style(theme::button(Kind::Secondary))
                .on_press(Msg::Stop)
        } else {
            let can_send = enabled && !self.input.text().trim().is_empty();
            button(icon(icon::PAPER_PLANE_RIGHT).size(14))
                .padding(9)
                .style(theme::button(Kind::Primary))
                .on_press_maybe(can_send.then_some(Msg::Send))
        };
        col = col
            .push(row![editor, action].spacing(8).align_y(Alignment::End))
            .push(
                container(widgets::label(
                    "Enter to send, Shift+Enter for new line",
                    10.0,
                    TEXT_MUTED,
                ))
                .center_x(Fill),
            );

        container(col).padding([14, 24]).center_x(Fill).into()
    }

    fn params_panel(&self) -> Element<'_, Msg> {
        let mut header = row![
            widgets::label("Parameters", SMALL, TEXT_SECONDARY).font(widgets::BOLD),
            space::horizontal()
        ];
        if self.params != ChatParams::default() {
            header = header.push(widgets::link(
                format!("{}  Reset", icon::ARROW_COUNTER_CLOCKWISE),
                TEXT_MUTED,
                Msg::ResetParams,
            ));
        }
        let param = |name: &'static str,
                     value: String,
                     control: Element<'static, Msg>|
         -> Element<'static, Msg> {
            row![
                column![widgets::muted(name), widgets::mono(value, 10.0, TEXT_MUTED)].width(110),
                control,
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        };
        widgets::card(
            Pad::Sm,
            column![
                header,
                param(
                    "Temperature",
                    format!("{:.1}", self.params.temperature),
                    slider(0.0..=2.0, self.params.temperature, Msg::Temperature)
                        .step(0.1_f32)
                        .into()
                ),
                param(
                    "Top P",
                    format!("{:.2}", self.params.top_p),
                    slider(0.0..=1.0, self.params.top_p, Msg::TopP)
                        .step(0.05_f32)
                        .into()
                ),
                param(
                    "Max Tokens",
                    self.params.max_tokens.to_string(),
                    widgets::input("2048", &self.max_tokens_text, Msg::MaxTokens)
                        .width(120)
                        .into()
                ),
            ]
            .spacing(10),
        )
        .into()
    }

    fn drawer(&self) -> Element<'_, Msg> {
        let q = self.drawer_search.to_lowercase();
        let filtered: Vec<&Conversation> = self
            .conversations
            .iter()
            .filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q))
            .collect();

        let mut list = column![].spacing(2);
        if !self.convs_loaded {
            list = list.push(
                container(widgets::muted("Loading..."))
                    .center_x(Fill)
                    .padding(24),
            );
        } else if filtered.is_empty() {
            list = list.push(
                container(widgets::muted(if q.is_empty() {
                    "No conversations yet."
                } else {
                    "No matches."
                }))
                .center_x(Fill)
                .padding(24),
            );
        }
        for conv in filtered {
            let active = self.active_conv.as_deref() == Some(conv.id.as_str());
            let color = if active { PAW_400 } else { TEXT_SECONDARY };
            let n = conv.message_count;
            let title: Element<'_, Msg> = match &self.renaming {
                Some((id, text)) if id == &conv.id => text_input("", text)
                    .on_input(Msg::RenameInput)
                    .on_submit(Msg::CommitRename)
                    .size(SMALL)
                    .padding(3)
                    .style(theme::input)
                    .into(),
                _ => widgets::label(conv.title.as_str(), SMALL, color).into(),
            };
            let row_content = row![
                icon(icon::CHAT_CIRCLE).color(color),
                column![
                    title,
                    widgets::label(
                        format!("{n} message{}", if n == 1 { "" } else { "s" }),
                        10.0,
                        TEXT_MUTED
                    )
                ]
                .width(Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            let actions = row![
                widgets::icon_button(
                    icon::PENCIL_SIMPLE,
                    "Rename",
                    Msg::StartRename(conv.id.clone())
                ),
                widgets::icon_button(icon::TRASH, "Delete", Msg::Delete(conv.id.clone())),
            ];
            let base = button(row_content)
                .width(Fill)
                .padding([6, 10])
                .style(theme::button(Kind::Row { selected: active }))
                .on_press(Msg::SelectConversation(conv.id.clone()));
            list = list.push(hover(
                base,
                container(actions)
                    .align_right(Fill)
                    .center_y(Fill)
                    .padding([0, 4]),
            ));
        }

        container(column![
            container(
                row![
                    widgets::overline("Conversations"),
                    space::horizontal(),
                    widgets::icon_button(icon::PLUS, "New conversation", Msg::NewChat),
                    widgets::icon_button(icon::X, "Close", Msg::ToggleDrawer),
                ]
                .align_y(Alignment::Center)
            )
            .padding([8, 16]),
            rule::horizontal(1),
            container(widgets::input(
                "Search...",
                &self.drawer_search,
                Msg::DrawerSearch
            ))
            .padding([8, 12]),
            rule::horizontal(1),
            scrollable(container(list).padding([4, 6])).height(Fill),
        ])
        .width(288)
        .height(Fill)
        .style(theme::page)
        .into()
    }
}

fn active_model_name(cx: &Cx<'_>) -> Option<String> {
    let handle = cx.shared.active_handle?;
    cx.shared
        .loaded
        .iter()
        .find(|m| m.handle_id == handle)
        .map(|m| model_display_name(&m.model_path))
}

async fn run_blocking<T: Send + 'static>(
    state: &Arc<AppState>,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| format!("Background task failed: {e}"))?
}

fn avatar<'a>(role: Role) -> Element<'a, Msg> {
    let (fill, color, glyph) = match role {
        Role::User => (tint(PAW_500, 0.15), PAW_400, icon::USER),
        _ => (SURFACE_OVERLAY, TEXT_MUTED, icon::ROBOT),
    };
    container(icon(glyph).size(12).color(color))
        .center(24)
        .style(theme::pill(fill, 12.0))
        .into()
}

/// Three pulsing dots while the model has not produced anything yet.
fn thinking_dots<'a>(since: Instant) -> Element<'a, Msg> {
    let t = since.elapsed().as_secs_f32();
    let mut dots = row![].spacing(6).align_y(Alignment::Center);
    for i in 0..3 {
        let phase = (t * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5;
        dots = dots.push(widgets::dot(tint(PAW_500, 0.3 + 0.7 * phase), 3.0));
    }
    container(dots).height(20).center_y(20).into()
}

/// Inline editor for a user message.
fn edit_box(content: &text_editor::Content) -> Element<'_, Msg> {
    row![
        avatar(Role::User),
        column![
            text_editor(content)
                .on_action(Msg::EditInput)
                .height(90)
                .padding(10)
                .size(BODY)
                .style(theme::editor),
            row![
                widgets::btn_sm(Kind::Primary, None, "Save & Submit", Some(Msg::SubmitEdit)),
                widgets::btn_sm(Kind::Ghost, None, "Cancel", Some(Msg::CancelEdit)),
            ]
            .spacing(8),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .into()
}
