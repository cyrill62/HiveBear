use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use hivebear_inference::types::ChatMessage;
use hivebear_persistence::{Conversation, MessageRole};

use crate::backend::{self, inference::ChatParams, inference::StreamEnd};
use crate::error::CmdResult;
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::util::model_display_name;
use crate::ui::widgets;
use crate::ui::{blocking, copy, rt, toast_err, Navigate, Prefs, Route};

const CONTENT_WIDTH: f32 = 672.;

const SUGGESTIONS: &[(&str, &str)] = &[
    ("Explain quantum computing", "Break it down in simple terms"),
    (
        "Write a Python sort function",
        "With type hints and docstring",
    ),
    ("Compare Rust vs Go", "Pros, cons, and use cases"),
];

#[derive(Clone, Copy, PartialEq)]
enum Role {
    System,
    User,
    Assistant,
}

struct Msg {
    role: Role,
    content: String,
    /// An error shown in place of a reply. Not sent back to the model.
    error: bool,
}

impl Msg {
    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            error: false,
        }
    }
}

enum StreamEvent {
    Token(String),
    ConversationCreated(Conversation),
    Done(CmdResult<StreamEnd>),
}

struct Streaming {
    cancel: Arc<AtomicBool>,
    _task: Task<()>,
}

pub struct Chat {
    state: Arc<AppState>,
    shared: Entity<Shared>,

    messages: Vec<Msg>,
    input: Entity<TextareaState>,
    scroll: ScrollHandle,

    params: ChatParams,
    params_open: bool,
    temperature: Entity<SliderState>,
    top_p: Entity<SliderState>,
    max_tokens: Entity<InputState>,

    drawer_open: bool,
    drawer_search: Entity<InputState>,
    renaming: Option<(String, Entity<InputState>)>,
    conversations: Vec<Conversation>,
    convs_loaded: bool,
    active_conv: Option<String>,

    editing: Option<(usize, Entity<TextareaState>)>,
    streaming: Option<Streaming>,
    show_nudge: bool,
    new_chat_requested: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Navigate> for Chat {}

impl Chat {
    pub fn new(
        state: Arc<AppState>,
        shared: Entity<Shared>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let params = ChatParams::default();
        // Enter sends; Shift+Enter is a new line.
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message HiveBear...")
                .auto_grow(1, 8)
                .submit_on_enter(true)
        });
        let temperature = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(2.)
                .step(0.1)
                .default_value(params.temperature)
        });
        let top_p = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(1.)
                .step(0.05)
                .default_value(params.top_p)
        });
        let max_tokens =
            cx.new(|cx| InputState::new(window, cx).default_value(params.max_tokens.to_string()));
        let drawer_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));

        let _subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.send_input(window, cx);
                }
            }),
            cx.subscribe(&temperature, |this, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(v) = event {
                    this.params.temperature = (v.start() * 10.).round() / 10.;
                    cx.notify();
                }
            }),
            cx.subscribe(&top_p, |this, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(v) = event {
                    this.params.top_p = (v.start() * 20.).round() / 20.;
                    cx.notify();
                }
            }),
            cx.subscribe(&max_tokens, |this, input, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    if let Ok(n) = input.read(cx).value().trim().parse::<u32>() {
                        this.params.max_tokens = n.clamp(128, 131_072);
                        cx.notify();
                    }
                }
            }),
            cx.subscribe(&drawer_search, |_, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            }),
        ];

        Self {
            state,
            shared,
            messages: vec![],
            input,
            scroll: ScrollHandle::new(),
            params,
            params_open: false,
            temperature,
            top_p,
            max_tokens,
            drawer_open: false,
            drawer_search,
            renaming: None,
            conversations: vec![],
            convs_loaded: false,
            active_conv: None,
            editing: None,
            streaming: None,
            show_nudge: false,
            new_chat_requested: false,
            _subscriptions,
        }
    }

    pub fn enter(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.shared.update(cx, |s, cx| {
            s.refresh_loaded(cx);
            s.refresh_installed(cx);
        });
        if !self.convs_loaded {
            self.refresh_conversations(cx);
        }
        if std::mem::take(&mut self.new_chat_requested) {
            self.new_chat(cx);
        }
    }

    /// Ctrl/⌘+N or the palette: start a fresh conversation.
    pub fn request_new_chat(&mut self, cx: &mut Context<Self>) {
        self.new_chat(cx);
    }

    fn refresh_conversations(&mut self, cx: &mut Context<Self>) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = blocking(rt, s, backend::chat::list_conversations).await;
            this.update(cx, |this, cx| {
                this.convs_loaded = true;
                match result {
                    Ok(c) => this.conversations = c,
                    Err(e) => toast_err(cx, format!("Couldn't load conversations: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn new_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &self.streaming {
            s.cancel.store(true, Ordering::Relaxed);
        }
        self.active_conv = None;
        self.messages.clear();
        self.editing = None;
        self.drawer_open = false;
        cx.notify();
    }

    fn select_conversation(&mut self, id: String, cx: &mut Context<Self>) {
        if self.streaming.is_some() {
            toast_err(cx, "Wait for the reply to finish.");
            return;
        }
        self.active_conv = Some(id.clone());
        self.editing = None;
        self.drawer_open = false;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = blocking(rt, s, move |s| {
                backend::chat::get_conversation_messages(s, &id)
            })
            .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(persisted) => {
                        this.messages = persisted
                            .into_iter()
                            .map(|m| {
                                let role = match m.role {
                                    MessageRole::User => Role::User,
                                    MessageRole::Assistant => Role::Assistant,
                                    MessageRole::System => Role::System,
                                };
                                Msg::new(role, m.content)
                            })
                            .collect();
                        this.scroll.scroll_to_bottom();
                    }
                    Err(e) => toast_err(cx, format!("Couldn't load messages: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn send_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if self.streaming.is_some() || text.is_empty() {
            return;
        }
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        self.send(text, true, cx);
    }

    /// Send `text` as the next user turn.
    ///
    /// `persist_user` is false for regenerate and edit-and-resend, which
    /// record only the new reply.
    fn send(&mut self, text: String, persist_user: bool, cx: &mut Context<Self>) {
        let (handle_id, model_name) = {
            let shared = self.shared.read(cx);
            let Some(handle_id) = shared.active_handle else {
                return;
            };
            let name = shared
                .loaded
                .iter()
                .find(|m| m.handle_id == handle_id)
                .map_or_else(|| "chat".to_string(), |m| model_display_name(&m.model_path));
            (handle_id, name)
        };
        if self.streaming.is_some() || text.trim().is_empty() {
            return;
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
        self.messages.push(Msg::new(Role::User, text.clone()));
        self.messages.push(Msg::new(Role::Assistant, ""));
        self.scroll.scroll_to_bottom();

        let create = persist_user && self.active_conv.is_none();
        let title = if text.chars().count() > 40 {
            format!("{}...", text.chars().take(40).collect::<String>())
        } else {
            text.clone()
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let state = Arc::clone(&self.state);
        let params = self.params;
        let mut conv_id = self.active_conv.clone();
        let rt = rt(cx);

        // Unbounded: a token must never be dropped because the UI is busy.
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        rt.spawn({
            let rt = rt.clone();
            async move {
                if create {
                    let (t, m) = (title, model_name);
                    match blocking(rt.clone(), Arc::clone(&state), move |s| {
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
                    let result = blocking(rt.clone(), Arc::clone(&state), move |s| {
                        backend::chat::add_message(s, &id, "user", &text)
                    })
                    .await;
                    if let Err(e) = result {
                        tracing::warn!("Failed to save message: {e}");
                    }
                }

                let token_tx = tx.clone();
                let end = backend::inference::stream_chat(
                    &state,
                    handle_id,
                    history,
                    params,
                    &stop,
                    |t| {
                        let _ = token_tx.unbounded_send(StreamEvent::Token(t.to_string()));
                    },
                )
                .await;

                if let (Ok(StreamEnd::Done(reply)), Some(id)) = (&end, conv_id) {
                    if !reply.is_empty() {
                        let reply = reply.clone();
                        let result = blocking(rt, Arc::clone(&state), move |s| {
                            backend::chat::add_message(s, &id, "assistant", &reply)
                        })
                        .await;
                        if let Err(e) = result {
                            tracing::warn!("Failed to save reply: {e}");
                        }
                    }
                }
                let _ = tx.unbounded_send(StreamEvent::Done(end));
            }
        });

        let task = cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                let alive = this
                    .update(cx, |this, cx| this.on_stream(event, cx))
                    .is_ok();
                if !alive {
                    return;
                }
            }
        });
        self.streaming = Some(Streaming {
            cancel,
            _task: task,
        });
        cx.notify();
    }

    fn on_stream(&mut self, event: StreamEvent, cx: &mut Context<Self>) {
        let stopped = self
            .streaming
            .as_ref()
            .is_some_and(|s| s.cancel.load(Ordering::Relaxed));
        match event {
            StreamEvent::Token(t) => {
                if !stopped {
                    if let Some(last) = self.messages.last_mut() {
                        last.content.push_str(&t);
                    }
                    self.scroll.scroll_to_bottom();
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
                            last.content = reply.clone();
                        }
                        if !reply.is_empty() && !cx.global::<Prefs>().first_chat_nudge_dismissed {
                            self.show_nudge = true;
                        }
                        None
                    }
                    Ok(StreamEnd::Failed(e)) | Err(e) => Some(e),
                };
                if let (Some(e), Some(last)) = (failure, self.messages.last_mut()) {
                    last.content = format!("Error: {e}");
                    last.error = true;
                }
                self.refresh_conversations(cx);
            }
        }
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &self.streaming {
            s.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    fn regenerate(&mut self, cx: &mut Context<Self>) {
        let n = self.messages.len();
        if n >= 2
            && self.messages[n - 1].role == Role::Assistant
            && self.messages[n - 2].role == Role::User
        {
            self.messages.pop();
            if let Some(user) = self.messages.pop() {
                self.send(user.content, false, cx);
            }
        }
    }

    fn start_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.messages.get(index).map(|m| m.content.clone()) else {
            return;
        };
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(text)
                .auto_grow(3, 10)
        });
        self.editing = Some((index, editor));
        cx.notify();
    }

    fn submit_edit(&mut self, cx: &mut Context<Self>) {
        if let Some((index, editor)) = self.editing.take() {
            let text = editor.read(cx).value().trim().to_string();
            self.messages.truncate(index);
            self.send(text, false, cx);
        }
    }

    fn start_rename(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .conversations
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        input.update(cx, |i, cx| i.focus(window, cx));
        let sub = cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
            InputEvent::PressEnter { .. } | InputEvent::Blur => this.commit_rename(cx),
            _ => {}
        });
        self._subscriptions.push(sub);
        self.renaming = Some((id, input));
        cx.notify();
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some((id, input)) = self.renaming.take() else {
            return;
        };
        let title = input.read(cx).value().trim().to_string();
        cx.notify();
        if title.is_empty() {
            return;
        }
        self.conversation_op(cx, move |s| {
            backend::chat::rename_conversation(s, &id, &title)
        });
    }

    fn delete_conversation(&mut self, id: String, cx: &mut Context<Self>) {
        if self.active_conv.as_deref() == Some(id.as_str()) {
            self.active_conv = None;
            self.messages.clear();
        }
        self.conversation_op(cx, move |s| backend::chat::delete_conversation(s, &id));
    }

    /// A rename or delete, followed by a list refresh.
    fn conversation_op(
        &mut self,
        cx: &mut Context<Self>,
        op: impl FnOnce(&AppState) -> CmdResult<()> + Send + 'static,
    ) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = blocking(rt, s, op).await;
            this.update(cx, |this, cx| {
                if let Err(e) = result {
                    toast_err(cx, e);
                }
                this.refresh_conversations(cx);
            })
            .ok();
        })
        .detach();
    }

    fn dismiss_nudge(&mut self, cx: &mut Context<Self>) {
        // Shown once: gone for good once clicked or dismissed.
        Prefs::update(cx, |p| p.first_chat_nudge_dismissed = true);
        self.show_nudge = false;
        cx.notify();
    }

    fn active_model_name(&self, cx: &App) -> Option<String> {
        let shared = self.shared.read(cx);
        let handle = shared.active_handle?;
        shared
            .loaded
            .iter()
            .find(|m| m.handle_id == handle)
            .map(|m| model_display_name(&m.model_path))
    }

    // ── Rendering ───────────────────────────────────────────────────

    fn no_model(&self, cx: &mut Context<Self>) -> Div {
        let shared = self.shared.read(cx);
        let busy = shared.loading_model.clone();
        let models: Vec<(String, String)> = shared
            .installed
            .iter()
            .take(4)
            .map(|m| (m.id.clone(), m.name.clone()))
            .collect();
        let error = shared.load_error.clone();
        let mut state = widgets::empty_state(
            IconName::Sparkles,
            "No model loaded",
            "Load an installed model to start chatting.",
        );
        if models.is_empty() {
            state = state.child(widgets::muted(
                "Install a model from the Model Browser first.",
            ));
        } else {
            state = state.child(h_flex().pt_1().gap_2().children(models.into_iter().map(
                |(id, name)| {
                    let loading = busy.as_deref() == Some(id.as_str());
                    widgets::button(SharedString::from(format!("chat-load-{id}")), name)
                        .outline()
                        .small()
                        .loading(loading)
                        .disabled(busy.is_some())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = id.clone();
                            this.shared.update(cx, |s, cx| s.load_model(&id, false, cx));
                        }))
                },
            )));
        }
        if let Some(e) = error {
            state = state.child(div().text_xs().text_color(c(DANGER)).child(e));
        }
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(state)
    }

    fn empty_conversation(&self, cx: &mut Context<Self>) -> Div {
        let heading = self.active_model_name(cx).map_or_else(
            || "What can I help you with?".to_string(),
            |m| format!("Chat with {m}"),
        );
        let mut col = v_flex()
            .items_center()
            .gap_1p5()
            .w(px(448.))
            .child(widgets::icon_tile(
                IconName::Sparkles,
                64.,
                ca(PAW_500, 0.1),
                c(PAW_500),
            ))
            .child(
                div()
                    .pt_2()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(heading),
            )
            .child(widgets::muted("Try a suggestion or type your own message."))
            .child(div().h_3());
        for (i, (title, desc)) in SUGGESTIONS.iter().enumerate() {
            let text = format!("{title} — {desc}");
            col = col.child(
                widgets::clickable_card(("suggestion", i))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .child(*title),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(c(TEXT_MUTED))
                            .child(*desc),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.send(text.clone(), true, cx))),
            );
        }
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(col)
    }

    fn message_list(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let model = self
            .active_model_name(cx)
            .unwrap_or_else(|| "Assistant".into());
        let streaming = self.streaming.is_some();
        let last = self.messages.len().saturating_sub(1);

        let mut list = v_flex().w_full().max_w(px(CONTENT_WIDTH)).gap_6();
        for (i, msg) in self.messages.iter().enumerate() {
            if let Some((idx, editor)) = &self.editing {
                if *idx == i && msg.role == Role::User {
                    list = list.child(
                        h_flex()
                            .items_start()
                            .gap_3()
                            .child(avatar(Role::User))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_2()
                                    .child(Textarea::new(editor))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                widgets::button("edit-submit", "Save & Submit")
                                                    .primary()
                                                    .small()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.submit_edit(cx)
                                                    })),
                                            )
                                            .child(
                                                widgets::button("edit-cancel", "Cancel")
                                                    .ghost()
                                                    .small()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.editing = None;
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            ),
                    );
                    continue;
                }
            }

            let is_streaming = streaming && i == last && msg.role == Role::Assistant;
            let who = match msg.role {
                Role::User => "You".to_string(),
                Role::System => "System".to_string(),
                Role::Assistant => model.clone(),
            };
            let body = if msg.error {
                div()
                    .text_sm()
                    .text_color(c(DANGER))
                    .child(msg.content.clone())
                    .into_any_element()
            } else if msg.role == Role::User {
                div()
                    .text_sm()
                    .child(msg.content.clone())
                    .into_any_element()
            } else if msg.content.is_empty() && is_streaming {
                thinking_dots().into_any_element()
            } else {
                v_flex()
                    .text_sm()
                    .child(TextView::markdown(
                        SharedString::from(format!("msg-{i}")),
                        msg.content.clone(),
                    ))
                    .when(is_streaming, |d| {
                        d.child(div().text_color(c(PAW_500)).child("▍"))
                    })
                    .into_any_element()
            };

            // Actions appear on hover, as in the web app.
            let group: SharedString = format!("msg-group-{i}").into();
            let mut actions = h_flex()
                .invisible()
                .group_hover(group.clone(), |s| s.visible());
            if !msg.content.is_empty() && !is_streaming {
                let content = msg.content.clone();
                actions = actions.child(
                    widgets::icon_button(("copy", i), IconName::Copy, "Copy")
                        .on_click(move |_, _, cx| copy(cx, content.clone())),
                );
                if msg.role == Role::User && !streaming {
                    actions = actions.child(
                        widgets::icon_button(("edit", i), IconName::Pencil, "Edit and resend")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_edit(i, window, cx)
                            })),
                    );
                }
                if msg.role == Role::Assistant && i == last && !streaming {
                    actions = actions.child(
                        widgets::icon_button(("regen", i), IconName::RefreshCw, "Regenerate")
                            .on_click(cx.listener(|this, _, _, cx| this.regenerate(cx))),
                    );
                }
            }

            list = list.child(
                h_flex()
                    .group(group)
                    .items_start()
                    .gap_3()
                    .child(avatar(msg.role))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .h(px(24.))
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(c(TEXT_MUTED))
                                            .child(who),
                                    )
                                    .child(actions),
                            )
                            .child(body),
                    ),
            );
        }

        div()
            .id("chat-messages")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(h_flex().w_full().justify_center().p_6().child(list))
    }

    fn input_area(&self, cx: &mut Context<Self>) -> Div {
        let streaming = self.streaming.is_some();
        let (loaded, active) = {
            let s = self.shared.read(cx);
            (s.loaded.clone(), s.active_handle)
        };
        let enabled = active.is_some();

        let mut col = v_flex().w_full().max_w(px(CONTENT_WIDTH)).gap_2();

        if self.show_nudge && !streaming {
            col = col.child(
                widgets::accent_card().px_4().py_2().child(
                    h_flex()
                        .gap_3()
                        .child(
                            Icon::new(IconName::Gauge)
                                .size(px(16.))
                                .text_color(c(PAW_500)),
                        )
                        .child(
                            div()
                                .id("nudge-benchmark")
                                .flex_1()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|s| s.text_color(c(PAW_400)))
                                .child("See how fast your machine is — run a 30s benchmark →")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dismiss_nudge(cx);
                                    cx.emit(Navigate(Route::Benchmark));
                                })),
                        )
                        .child(
                            widgets::button("nudge-mesh", "Join the hive")
                                .icon(IconName::Network)
                                .ghost()
                                .xsmall()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dismiss_nudge(cx);
                                    cx.emit(Navigate(Route::Mesh));
                                })),
                        )
                        .child(
                            widgets::icon_button("nudge-dismiss", IconName::X, "Dismiss")
                                .on_click(cx.listener(|this, _, _, cx| this.dismiss_nudge(cx))),
                        ),
                ),
            );
        }

        // Model selector (one pill per loaded model) and the parameters toggle.
        let mut models = h_flex().gap_1p5();
        for m in &loaded {
            let selected = Some(m.handle_id) == active;
            let handle = m.handle_id;
            let label = if loaded.len() > 1 {
                format!("{} ({})", model_display_name(&m.model_path), m.engine)
            } else {
                model_display_name(&m.model_path)
            };
            models = models.child(
                h_flex()
                    .id(("model-pill", handle as usize))
                    .gap_1p5()
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .text_color(c(if selected { TEXT_SECONDARY } else { TEXT_MUTED }))
                    .bg(c(if selected { SURFACE_RAISED } else { SURFACE }))
                    .border_1()
                    .border_color(if selected {
                        ca(PAW_500, 0.3)
                    } else {
                        c(BORDER)
                    })
                    .when(selected, |d| d.child(widgets::dot(c(SUCCESS), 6.)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.shared.update(cx, |s, cx| {
                            s.active_handle = Some(handle);
                            cx.notify();
                        });
                    })),
            );
        }
        let params_active = self.params_open || self.params != ChatParams::default();
        col = col.child(
            h_flex().justify_between().child(models).child(
                widgets::icon_button(
                    "params-toggle",
                    IconName::SlidersHorizontal,
                    "Chat parameters",
                )
                .when(params_active, |b| b.text_color(c(PAW_400)))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.params_open = !this.params_open;
                    cx.notify();
                })),
            ),
        );

        if self.params_open {
            col = col.child(self.params_panel(cx));
        }

        let action = if streaming {
            widgets::button("stop", "")
                .icon(IconName::Square)
                .outline()
                .tooltip("Stop generating")
                .on_click(cx.listener(|this, _, _, cx| this.stop(cx)))
        } else {
            widgets::button("send", "")
                .icon(IconName::Send)
                .primary()
                .tooltip("Send (Enter)")
                .disabled(!enabled)
                .on_click(cx.listener(|this, _, window, cx| this.send_input(window, cx)))
        };
        col = col
            .child(
                h_flex()
                    .items_end()
                    .gap_2()
                    .child(div().flex_1().child(Textarea::new(&self.input)))
                    .child(action),
            )
            .child(
                div()
                    .text_center()
                    .text_size(px(10.))
                    .text_color(c(TEXT_MUTED))
                    .child("Enter to send, Shift+Enter for new line"),
            );

        h_flex()
            .w_full()
            .justify_center()
            .px_6()
            .py_4()
            .border_t_1()
            .border_color(c(BORDER))
            .child(col)
    }

    fn params_panel(&self, cx: &mut Context<Self>) -> Div {
        let row = |name: &'static str, value: String, control: AnyElement| {
            h_flex()
                .gap_3()
                .child(
                    v_flex()
                        .w(px(110.))
                        .child(widgets::muted(name))
                        .child(widgets::mono(value, TEXT_MUTED)),
                )
                .child(div().flex_1().child(control))
        };
        widgets::card()
            .p_3()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(TEXT_SECONDARY))
                            .child("Parameters"),
                    )
                    .when(self.params != ChatParams::default(), |d| {
                        d.child(
                            widgets::button("params-reset", "Reset")
                                .icon(IconName::RotateCcw)
                                .ghost()
                                .xsmall()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.params = ChatParams::default();
                                    let p = this.params;
                                    this.temperature
                                        .update(cx, |s, cx| s.set_value(p.temperature, window, cx));
                                    this.top_p
                                        .update(cx, |s, cx| s.set_value(p.top_p, window, cx));
                                    this.max_tokens.update(cx, |s, cx| {
                                        s.set_value(p.max_tokens.to_string(), window, cx)
                                    });
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                v_flex()
                    .pt_2()
                    .gap_3()
                    .child(row(
                        "Temperature",
                        format!("{:.1}", self.params.temperature),
                        Slider::new(&self.temperature).into_any_element(),
                    ))
                    .child(row(
                        "Top P",
                        format!("{:.2}", self.params.top_p),
                        Slider::new(&self.top_p).into_any_element(),
                    ))
                    .child(row(
                        "Max Tokens",
                        self.params.max_tokens.to_string(),
                        Input::new(&self.max_tokens).small().into_any_element(),
                    )),
            )
    }

    fn drawer(&self, cx: &mut Context<Self>) -> Div {
        let q = self.drawer_search.read(cx).value().to_lowercase();
        let filtered: Vec<&Conversation> = self
            .conversations
            .iter()
            .filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q))
            .collect();

        let mut list = v_flex().gap_0p5();
        if !self.convs_loaded {
            list = list.child(
                div()
                    .py_8()
                    .text_center()
                    .child(widgets::muted("Loading...")),
            );
        } else if filtered.is_empty() {
            list = list.child(
                div()
                    .py_8()
                    .text_center()
                    .child(widgets::muted(if q.is_empty() {
                        "No conversations yet."
                    } else {
                        "No matches."
                    })),
            );
        }
        for conv in filtered {
            let active = self.active_conv.as_deref() == Some(conv.id.as_str());
            let n = conv.message_count;
            let id = conv.id.clone();
            let group: SharedString = format!("conv-{}", conv.id).into();
            let title = match &self.renaming {
                Some((rid, input)) if rid == &conv.id => {
                    Input::new(input).small().into_any_element()
                }
                _ => div()
                    .text_xs()
                    .truncate()
                    .child(conv.title.clone())
                    .into_any_element(),
            };
            let (rename_id, delete_id) = (id.clone(), id.clone());
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("conv-row-{}", conv.id)))
                    .group(group.clone())
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_color(c(if active { PAW_400 } else { TEXT_SECONDARY }))
                    .when(active, |d| d.bg(ca(PAW_500, 0.1)))
                    .hover(|s| s.bg(c(SURFACE_RAISED)))
                    .child(Icon::new(IconName::MessageSquare).size(px(14.)))
                    .child(
                        v_flex().flex_1().min_w_0().child(title).child(
                            div()
                                .text_size(px(10.))
                                .text_color(c(TEXT_MUTED))
                                .child(format!("{n} message{}", if n == 1 { "" } else { "s" })),
                        ),
                    )
                    .child(
                        h_flex()
                            .invisible()
                            .group_hover(group, |s| s.visible())
                            .child(
                                widgets::icon_button(
                                    SharedString::from(format!("rename-{id}")),
                                    IconName::Pencil,
                                    "Rename",
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.start_rename(rename_id.clone(), window, cx);
                                    },
                                )),
                            )
                            .child(
                                widgets::icon_button(
                                    SharedString::from(format!("delete-{id}")),
                                    IconName::Trash,
                                    "Delete",
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.delete_conversation(delete_id.clone(), cx);
                                    },
                                )),
                            ),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_conversation(id.clone(), cx)),
                    ),
            );
        }

        v_flex()
            .w(px(288.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(c(BORDER))
            .child(
                h_flex()
                    .justify_between()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(c(BORDER))
                    .child(widgets::overline("Conversations"))
                    .child(
                        h_flex()
                            .child(
                                widgets::icon_button(
                                    "drawer-new",
                                    IconName::Plus,
                                    "New conversation",
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.new_chat(cx))),
                            )
                            .child(
                                widgets::icon_button("drawer-close", IconName::X, "Close")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.drawer_open = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(c(BORDER))
                    .child(Input::new(&self.drawer_search).small()),
            )
            .child(
                div()
                    .id("drawer-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_1p5()
                    .child(list),
            )
    }
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .active_conv
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .map_or(
                if self.active_conv.is_some() {
                    "Chat"
                } else {
                    "New Chat"
                }
                .to_string(),
                |c| c.title.clone(),
            );

        let top = h_flex()
            .justify_between()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(c(BORDER))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        widgets::icon_button(
                            "drawer-toggle",
                            IconName::PanelLeftOpen,
                            "Conversations",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.drawer_open = !this.drawer_open;
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(title),
                    ),
            )
            .child(
                widgets::icon_button("new-chat", IconName::Plus, "New chat")
                    .on_click(cx.listener(|this, _, _, cx| this.new_chat(cx))),
            );

        let has_loaded = !self.shared.read(cx).loaded.is_empty();
        let body = if !has_loaded {
            self.no_model(cx).into_any_element()
        } else if self.messages.is_empty() {
            self.empty_conversation(cx).into_any_element()
        } else {
            self.message_list(cx).into_any_element()
        };

        let main = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(top)
            .child(div().flex_1().min_h_0().child(body))
            .when(has_loaded, |d| d.child(self.input_area(cx)));

        h_flex()
            .size_full()
            .when(self.drawer_open, |d| d.child(self.drawer(cx)))
            .child(main)
    }
}

fn avatar(role: Role) -> Div {
    let (fill, color, icon) = match role {
        Role::User => (ca(PAW_500, 0.15), c(PAW_400), IconName::User),
        _ => (c(SURFACE_OVERLAY), c(TEXT_MUTED), IconName::Bot),
    };
    div()
        .mt_0p5()
        .size(px(24.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(fill)
        .child(Icon::new(icon).size(px(12.)).text_color(color))
}

/// Three pulsing dots while the model has not produced anything yet.
fn thinking_dots() -> impl IntoElement {
    h_flex().h(px(20.)).gap_1p5().children((0..3usize).map(|i| {
        widgets::dot(c(PAW_500), 6.).with_animation(
            ("thinking-dot", i),
            Animation::new(Duration::from_millis(1200)).repeat(),
            move |dot, t| {
                let phase = ((t * std::f32::consts::TAU) - i as f32 * 0.6).sin() * 0.5 + 0.5;
                dot.opacity(0.3 + 0.7 * phase)
            },
        )
    }))
}
