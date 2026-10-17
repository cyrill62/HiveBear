use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    RichText, Stroke, Ui, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_phosphor::regular as icon;
use hivebear_inference::types::ChatMessage;
use hivebear_persistence::{Conversation, MessageRole, PersistedMessage};
use tokio::sync::mpsc;

use crate::backend::{self, inference::ChatParams, inference::StreamEnd};
use crate::error::CmdResult;
use crate::state::AppState;
use crate::tasks::{poll_slot, Task};
use crate::ui::theme::*;
use crate::ui::util::model_display_name;
use crate::ui::widgets::{self, Btn, Pad};
use crate::ui::{Env, Route};

const CONTENT_WIDTH: f32 = 672.0;
const INPUT_ID: &str = "chat-input";

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

enum Event {
    Token(String),
    ConversationCreated(Conversation),
}

struct Stream {
    rx: mpsc::UnboundedReceiver<Event>,
    cancel: Arc<AtomicBool>,
    task: Task<CmdResult<StreamEnd>>,
}

enum MsgAction {
    Edit(usize),
    Regenerate,
}

#[derive(Default)]
pub struct Chat {
    messages: Vec<Msg>,
    input: String,
    params: ChatParams,
    params_open: bool,

    drawer_open: bool,
    drawer_search: String,
    renaming: Option<(String, String)>,
    conversations: Vec<Conversation>,
    convs_loaded: bool,
    convs_task: Option<Task<CmdResult<Vec<Conversation>>>>,
    active_conv: Option<String>,
    msgs_task: Option<Task<CmdResult<Vec<PersistedMessage>>>>,
    /// Rename/delete calls whose only follow-up is a list refresh.
    conv_ops: Vec<Task<CmdResult<()>>>,

    editing: Option<(usize, String)>,
    stream: Option<Stream>,
    show_nudge: bool,
    new_chat_requested: bool,
    md_cache: CommonMarkCache,
}

impl Chat {
    pub fn enter(&mut self, env: &mut Env) {
        env.shared.refresh_loaded(env.ctx);
        env.shared.refresh_installed(env.ctx);
        if !self.convs_loaded {
            self.refresh_conversations(env);
        }
        if std::mem::take(&mut self.new_chat_requested) {
            self.new_chat(env);
        }
    }

    /// Ctrl/⌘+N: start a fresh conversation on the next frame.
    pub fn request_new_chat(&mut self) {
        self.new_chat_requested = true;
    }

    fn refresh_conversations(&mut self, env: &Env) {
        self.convs_task = Some(env.ctx.blocking(backend::chat::list_conversations));
    }

    fn streaming(&self) -> bool {
        self.stream.is_some()
    }

    fn new_chat(&mut self, _env: &Env) {
        if let Some(s) = &self.stream {
            s.cancel.store(true, Ordering::Relaxed);
        }
        self.active_conv = None;
        self.messages.clear();
        self.editing = None;
        self.drawer_open = false;
    }

    fn select_conversation(&mut self, env: &Env, id: String) {
        let conv = id.clone();
        self.msgs_task = Some(
            env.ctx
                .blocking(move |s| backend::chat::get_conversation_messages(s, &conv)),
        );
        self.active_conv = Some(id);
        self.editing = None;
        self.drawer_open = false;
    }

    /// Send `text` as the next user turn.
    ///
    /// `persist_user` is false for regenerate and edit-and-resend, which
    /// record only the new reply.
    fn send(&mut self, env: &Env, text: String, persist_user: bool) {
        let Some(handle_id) = env.shared.active_handle else {
            return;
        };
        if self.streaming() || text.trim().is_empty() {
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

        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let egui_ctx = env.ctx.egui.clone();
        let params = self.params;
        let mut conv_id = self.active_conv.clone();

        let task = env.ctx.run(move |s| async move {
            if create {
                let (t, m) = (title, model_name);
                match blocking(&s, move |s| backend::chat::create_conversation(s, &t, &m)).await {
                    Ok(conv) => {
                        conv_id = Some(conv.id.clone());
                        let _ = tx.send(Event::ConversationCreated(conv));
                        egui_ctx.request_repaint();
                    }
                    Err(e) => tracing::warn!("Failed to create conversation: {e}"),
                }
            }
            if let (true, Some(id)) = (persist_user, conv_id.clone()) {
                if let Err(e) = blocking(&s, move |s| {
                    backend::chat::add_message(s, &id, "user", &text)
                })
                .await
                {
                    tracing::warn!("Failed to save message: {e}");
                }
            }

            let end = backend::inference::stream_chat(&s, handle_id, history, params, &stop, |t| {
                let _ = tx.send(Event::Token(t.to_string()));
                egui_ctx.request_repaint();
            })
            .await;

            if let (Ok(StreamEnd::Done(reply)), Some(id)) = (&end, conv_id) {
                if !reply.is_empty() {
                    let reply = reply.clone();
                    if let Err(e) = blocking(&s, move |s| {
                        backend::chat::add_message(s, &id, "assistant", &reply)
                    })
                    .await
                    {
                        tracing::warn!("Failed to save reply: {e}");
                    }
                }
            }
            end
        });

        self.stream = Some(Stream { rx, cancel, task });
    }

    fn stop(&mut self) {
        if let Some(s) = &self.stream {
            s.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn regenerate(&mut self, env: &Env) {
        let n = self.messages.len();
        if n < 2
            || self.messages[n - 1].role != Role::Assistant
            || self.messages[n - 2].role != Role::User
        {
            return;
        }
        self.messages.pop();
        let Some(user) = self.messages.pop() else {
            return;
        };
        self.send(env, user.content, false);
    }

    fn edit_and_resend(&mut self, env: &Env, index: usize, text: String) {
        self.messages.truncate(index);
        self.send(env, text, false);
    }

    pub fn poll(&mut self, env: &mut Env) {
        if std::mem::take(&mut self.new_chat_requested) {
            self.new_chat(env);
        }

        if let Some(result) = poll_slot(&mut self.convs_task) {
            self.convs_loaded = true;
            match result {
                Ok(c) => self.conversations = c,
                Err(e) => env
                    .toasts
                    .error(format!("Couldn't load conversations: {e}")),
            }
        }
        if let Some(result) = poll_slot(&mut self.msgs_task) {
            match result {
                Ok(persisted) => {
                    self.messages = persisted
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
                }
                Err(e) => env.toasts.error(format!("Couldn't load messages: {e}")),
            }
        }

        let mut ops_done = false;
        self.conv_ops.retain_mut(|t| match t.poll() {
            None => true,
            Some(r) => {
                if let Ok(Err(e)) = r {
                    env.toasts.error(e);
                }
                ops_done = true;
                false
            }
        });
        if ops_done {
            self.refresh_conversations(env);
        }

        let Some(stream) = &mut self.stream else {
            return;
        };
        let stopped = stream.cancel.load(Ordering::Relaxed);
        while let Ok(event) = stream.rx.try_recv() {
            match event {
                Event::Token(t) if !stopped => {
                    if let Some(last) = self.messages.last_mut() {
                        last.content.push_str(&t);
                    }
                }
                Event::Token(_) => {}
                Event::ConversationCreated(conv) => {
                    self.active_conv = Some(conv.id.clone());
                    self.conversations.insert(0, conv);
                }
            }
        }

        let Some(result) = stream.task.poll() else {
            return;
        };
        self.stream = None;
        let failure = match result {
            Ok(Ok(StreamEnd::Done(reply))) => {
                if let Some(last) = self.messages.last_mut() {
                    last.content = reply.clone();
                }
                if !reply.is_empty() && !env.prefs.first_chat_nudge_dismissed {
                    self.show_nudge = true;
                }
                None
            }
            Ok(Ok(StreamEnd::Failed(e))) | Ok(Err(e)) => Some(e),
            Err(()) => Some("generation stopped unexpectedly".into()),
        };
        if let (Some(e), Some(last)) = (failure, self.messages.last_mut()) {
            last.content = format!("Error: {e}");
            last.error = true;
        }
        self.refresh_conversations(env);
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        if self.streaming() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }

        if self.drawer_open {
            self.drawer(ui, env);
        }

        // Top bar
        egui::Panel::top("chat-top")
            .frame(
                Frame::new()
                    .fill(SURFACE)
                    .inner_margin(Margin::symmetric(16, 8))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, icon::SIDEBAR_SIMPLE, "Conversations").clicked() {
                        self.drawer_open = !self.drawer_open;
                    }
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
                    ui.add(egui::Label::new(widgets::text(title).strong()).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::icon_button(ui, icon::PLUS, "New chat").clicked() {
                            self.new_chat(env);
                        }
                    });
                });
            });

        let has_loaded = !env.shared.loaded.is_empty();
        if has_loaded {
            egui::Panel::bottom("chat-input")
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .inner_margin(Margin::symmetric(24, 14))
                        .stroke(Stroke::new(1.0, BORDER)),
                )
                .show(ui, |ui| self.input_area(ui, env));
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(SURFACE))
            .show(ui, |ui| {
                if !has_loaded {
                    self.no_model(ui, env);
                } else if self.messages.is_empty() {
                    self.empty_conversation(ui, env);
                } else {
                    self.message_list(ui, env);
                }
            });
    }

    fn no_model(&mut self, ui: &mut Ui, env: &mut Env) {
        let mut load = None;
        ui.add_space((ui.available_height() / 2.0 - 120.0).max(0.0));
        widgets::empty_state(
            ui,
            icon::SPARKLE,
            "No model loaded",
            "Load an installed model to start chatting.",
            |ui| {
                if env.shared.installed.is_empty() {
                    ui.label(widgets::muted(
                        "Install a model from the Model Browser first.",
                    ));
                } else {
                    let busy_any = env.shared.model_loading().is_some();
                    ui.horizontal_wrapped(|ui| {
                        for m in env.shared.installed.iter().take(4) {
                            let busy = env.shared.model_loading() == Some(m.id.as_str());
                            if Btn::secondary("", &m.name)
                                .small()
                                .enabled(!busy_any)
                                .busy(busy)
                                .show(ui)
                                .clicked()
                            {
                                load = Some(m.id.clone());
                            }
                        }
                    });
                }
                if let Some(e) = &env.shared.load_error {
                    ui.label(RichText::new(e).size(SMALL).color(DANGER));
                }
            },
        );
        if let Some(id) = load {
            env.shared.load_model(env.ctx, &id, false);
        }
    }

    fn empty_conversation(&mut self, ui: &mut Ui, env: &mut Env) {
        let model = active_model_name(env);
        let mut picked = None;
        ui.add_space((ui.available_height() / 2.0 - 160.0).max(0.0));
        ui.vertical_centered(|ui| {
            widgets::icon_tile(ui, icon::SPARKLE, 64.0, tint(PAW_500, 0.1), PAW_500);
            ui.add_space(8.0);
            let heading = model.as_ref().map_or_else(
                || "What can I help you with?".to_string(),
                |m| format!("Chat with {m}"),
            );
            ui.label(widgets::text(heading).strong());
            ui.label(widgets::muted("Try a suggestion or type your own message."));
            ui.add_space(12.0);
            ui.set_max_width(448.0);
            for (title, desc) in SUGGESTIONS {
                let response = widgets::clickable_card(ui, *title, Pad::Sm, |ui| {
                    ui.label(
                        RichText::new(*title)
                            .size(SMALL)
                            .strong()
                            .color(TEXT_PRIMARY),
                    );
                    ui.label(RichText::new(*desc).size(TINY).color(TEXT_MUTED));
                });
                if response.clicked() {
                    picked = Some(format!("{title} — {desc}"));
                }
            }
        });
        if let Some(text) = picked {
            self.send(env, text, true);
        }
    }

    fn message_list(&mut self, ui: &mut Ui, env: &mut Env) {
        let model = active_model_name(env).unwrap_or_else(|| "Assistant".into());
        let streaming = self.streaming();
        let last = self.messages.len().saturating_sub(1);
        let mut action = None;
        let mut submit_edit = None;
        let mut cancel_edit = false;

        egui::ScrollArea::vertical()
            .id_salt("chat-messages")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let width = ui.available_width();
                let content = (width - 48.0).min(CONTENT_WIDTH);
                ui.add_space(24.0);
                ui.horizontal_top(|ui| {
                    ui.add_space((width - content) / 2.0);
                    ui.vertical(|ui| {
                        ui.set_width(content);
                        ui.spacing_mut().item_spacing.y = 20.0;
                        for (i, msg) in self.messages.iter().enumerate() {
                            ui.push_id(i, |ui| {
                                if let Some((idx, text)) = &mut self.editing {
                                    if *idx == i && msg.role == Role::User {
                                        match edit_box(ui, text) {
                                            Some(true) => submit_edit = Some((i, text.clone())),
                                            Some(false) => cancel_edit = true,
                                            None => {}
                                        }
                                        return;
                                    }
                                }
                                let is_streaming =
                                    streaming && i == last && msg.role == Role::Assistant;
                                let can_edit = msg.role == Role::User && !streaming;
                                let can_regen =
                                    msg.role == Role::Assistant && i == last && !streaming;
                                if let Some(a) = message_row(
                                    ui,
                                    msg,
                                    &model,
                                    is_streaming,
                                    can_edit,
                                    can_regen,
                                    i,
                                    &mut self.md_cache,
                                    env,
                                ) {
                                    action = Some(a);
                                }
                            });
                        }
                    });
                });
                ui.add_space(24.0);
            });

        if cancel_edit {
            self.editing = None;
        }
        if let Some((i, text)) = submit_edit {
            self.editing = None;
            self.edit_and_resend(env, i, text);
        }
        match action {
            Some(MsgAction::Edit(i)) => self.editing = Some((i, self.messages[i].content.clone())),
            Some(MsgAction::Regenerate) => self.regenerate(env),
            None => {}
        }
    }

    fn input_area(&mut self, ui: &mut Ui, env: &mut Env) {
        let side = ((ui.available_width() - CONTENT_WIDTH) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(side);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - side);

                if self.show_nudge && !self.streaming() {
                    self.nudge(ui, env);
                    ui.add_space(6.0);
                }

                // Model selector + parameters
                ui.horizontal(|ui| {
                    if env.shared.loaded.len() > 1 {
                        let current = env.shared.active_handle;
                        let label = active_model_name(env).unwrap_or_default();
                        egui::ComboBox::from_id_salt("chat-model")
                            .selected_text(RichText::new(label).size(TINY))
                            .show_ui(ui, |ui| {
                                for m in &env.shared.loaded {
                                    let text = format!(
                                        "{} ({})",
                                        model_display_name(&m.model_path),
                                        m.engine
                                    );
                                    if ui
                                        .selectable_label(current == Some(m.handle_id), text)
                                        .clicked()
                                    {
                                        env.shared.active_handle = Some(m.handle_id);
                                    }
                                }
                            });
                    } else if let Some(name) = active_model_name(env) {
                        Frame::new()
                            .fill(SURFACE_RAISED)
                            .corner_radius(CornerRadius::same(RADIUS_SM))
                            .inner_margin(Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    widgets::dot(ui, SUCCESS, 3.0);
                                    ui.label(RichText::new(name).size(TINY).color(TEXT_SECONDARY));
                                });
                            });
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let non_default = self.params != ChatParams::default();
                        let color = if self.params_open || non_default {
                            PAW_400
                        } else {
                            TEXT_MUTED
                        };
                        let b = egui::Button::new(RichText::new(icon::SLIDERS).color(color))
                            .frame_when_inactive(false);
                        if ui.add(b).on_hover_text("Chat parameters").clicked() {
                            self.params_open = !self.params_open;
                        }
                    });
                });

                if self.params_open {
                    self.params_panel(ui);
                }

                // Input box
                let input_id = egui::Id::new(INPUT_ID);
                let enabled = env.shared.active_handle.is_some();
                let enter = ui.memory(|m| m.has_focus(input_id))
                    && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                let mut send = false;
                Frame::new()
                    .fill(SURFACE_RAISED)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(RADIUS_LG))
                    .inner_margin(Margin::symmetric(4, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("chat-input-scroll")
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    ui.add_enabled(
                                        enabled,
                                        egui::TextEdit::multiline(&mut self.input)
                                            .id(input_id)
                                            .hint_text(
                                                RichText::new("Message HiveBear...")
                                                    .color(TEXT_MUTED),
                                            )
                                            .desired_rows(1)
                                            .frame(Frame::NONE)
                                            .return_key(Some(KeyboardShortcut::new(
                                                Modifiers::SHIFT,
                                                Key::Enter,
                                            )))
                                            .desired_width(ui.available_width() - 44.0)
                                            .margin(Margin::symmetric(10, 8)),
                                    );
                                });
                            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                                if self.streaming() {
                                    let b = egui::Button::new(
                                        RichText::new(icon::STOP).color(TEXT_SECONDARY),
                                    )
                                    .fill(SURFACE_OVERLAY)
                                    .corner_radius(CornerRadius::same(RADIUS_MD))
                                    .min_size(Vec2::splat(32.0));
                                    if ui.add(b).on_hover_text("Stop generating").clicked() {
                                        self.stop();
                                    }
                                } else {
                                    let can_send = enabled && !self.input.trim().is_empty();
                                    let b = egui::Button::new(
                                        RichText::new(icon::PAPER_PLANE_RIGHT)
                                            .color(Color32::WHITE),
                                    )
                                    .fill(if can_send {
                                        PAW_500
                                    } else {
                                        tint(PAW_500, 0.3)
                                    })
                                    .corner_radius(CornerRadius::same(RADIUS_MD))
                                    .min_size(Vec2::splat(32.0));
                                    if ui
                                        .add_enabled(can_send, b)
                                        .on_hover_text("Send (Enter)")
                                        .clicked()
                                    {
                                        send = true;
                                    }
                                }
                            });
                        });
                    });
                if (send || enter) && !self.streaming() && !self.input.trim().is_empty() {
                    let text = std::mem::take(&mut self.input).trim().to_string();
                    self.send(env, text, true);
                    ui.memory_mut(|m| m.request_focus(input_id));
                }

                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Enter to send, Shift+Enter for new line")
                            .size(10.0)
                            .color(TEXT_MUTED),
                    );
                });
            });
        });
    }

    fn params_panel(&mut self, ui: &mut Ui) {
        widgets::card(ui, Pad::Sm, |ui| {
            ui.horizontal(|ui| {
                ui.label(widgets::secondary("Parameters").size(SMALL).strong());
                if self.params != ChatParams::default() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::link_button(
                            ui,
                            &format!("{}  Reset", icon::ARROW_COUNTER_CLOCKWISE),
                            TEXT_MUTED,
                        )
                        .clicked()
                        {
                            self.params = ChatParams::default();
                        }
                    });
                }
            });
            egui::Grid::new("chat-params")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(widgets::muted("Temperature"));
                    ui.add(
                        egui::Slider::new(&mut self.params.temperature, 0.0..=2.0)
                            .step_by(0.1)
                            .fixed_decimals(1),
                    );
                    ui.end_row();
                    ui.label(widgets::muted("Top P"));
                    ui.add(
                        egui::Slider::new(&mut self.params.top_p, 0.0..=1.0)
                            .step_by(0.05)
                            .fixed_decimals(2),
                    );
                    ui.end_row();
                    ui.label(widgets::muted("Max Tokens"));
                    ui.add(
                        egui::DragValue::new(&mut self.params.max_tokens)
                            .range(128..=131_072)
                            .speed(64),
                    );
                    ui.end_row();
                });
        });
    }

    fn nudge(&mut self, ui: &mut Ui, env: &mut Env) {
        let mut go = None;
        let mut dismiss = false;
        Frame::new()
            .fill(tint(PAW_500, 0.05))
            .stroke(Stroke::new(1.0, tint(PAW_500, 0.3)))
            .corner_radius(CornerRadius::same(RADIUS_LG))
            .inner_margin(Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::GAUGE).color(PAW_500));
                    if widgets::link_button(
                        ui,
                        "See how fast your machine is — run a 30s benchmark →",
                        TEXT_PRIMARY,
                    )
                    .clicked()
                    {
                        go = Some(Route::Benchmark);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::icon_button(ui, icon::X, "Dismiss").clicked() {
                            dismiss = true;
                        }
                        if widgets::link_button(
                            ui,
                            &format!("{}  Join the hive", icon::NETWORK),
                            TEXT_MUTED,
                        )
                        .clicked()
                        {
                            go = Some(Route::Mesh);
                        }
                    });
                });
            });
        // Shown once: gone for good once clicked or dismissed.
        if dismiss || go.is_some() {
            env.prefs.first_chat_nudge_dismissed = true;
            self.show_nudge = false;
        }
        if let Some(route) = go {
            env.go(route);
        }
    }

    fn drawer(&mut self, ui: &mut Ui, env: &mut Env) {
        let streaming = self.streaming();
        let mut select = None;
        let mut delete = None;
        let mut rename = None;
        let mut new_chat = false;

        egui::Panel::left("chat-drawer")
            .exact_size(288.0)
            .resizable(false)
            .frame(Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, BORDER)))
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            widgets::overline(ui, "Conversations");
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::icon_button(ui, icon::X, "Close").clicked() {
                                    self.drawer_open = false;
                                }
                                if widgets::icon_button(ui, icon::PLUS, "New conversation")
                                    .clicked()
                                {
                                    new_chat = true;
                                }
                            });
                        });
                    });
                ui.separator();
                Frame::new()
                    .inner_margin(Margin::symmetric(12, 4))
                    .show(ui, |ui| {
                        widgets::search_field(
                            ui,
                            &mut self.drawer_search,
                            "Search...",
                            ui.available_width(),
                        );
                    });
                ui.separator();

                let q = self.drawer_search.to_lowercase();
                let filtered: Vec<&Conversation> = self
                    .conversations
                    .iter()
                    .filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q))
                    .collect();

                egui::ScrollArea::vertical()
                    .id_salt("chat-drawer-list")
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(Margin::symmetric(6, 4))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if !self.convs_loaded {
                                    ui.vertical_centered(|ui| {
                                        ui.label(widgets::muted("Loading..."))
                                    });
                                } else if filtered.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(24.0);
                                        ui.label(widgets::muted(if q.is_empty() {
                                            "No conversations yet."
                                        } else {
                                            "No matches."
                                        }));
                                    });
                                }
                                for conv in filtered {
                                    let active =
                                        self.active_conv.as_deref() == Some(conv.id.as_str());
                                    ui.push_id(&conv.id, |ui| {
                                        let fill = if active {
                                            tint(PAW_500, 0.1)
                                        } else {
                                            Color32::TRANSPARENT
                                        };
                                        let row = Frame::new()
                                            .fill(fill)
                                            .corner_radius(CornerRadius::same(RADIUS_MD))
                                            .inner_margin(Margin::symmetric(10, 6))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.horizontal(|ui| {
                                                    let color = if active {
                                                        PAW_400
                                                    } else {
                                                        TEXT_SECONDARY
                                                    };
                                                    ui.label(
                                                        RichText::new(icon::CHAT_CIRCLE)
                                                            .color(color),
                                                    );
                                                    ui.vertical(|ui| {
                                                        ui.set_max_width(
                                                            ui.available_width() - 56.0,
                                                        );
                                                        ui.spacing_mut().item_spacing.y = 0.0;
                                                        match &mut self.renaming {
                                                            Some((id, text)) if id == &conv.id => {
                                                                let r = ui.add(
                                                                    egui::TextEdit::singleline(
                                                                        text,
                                                                    )
                                                                    .desired_width(
                                                                        ui.available_width(),
                                                                    ),
                                                                );
                                                                r.request_focus();
                                                                if ui.input(|i| {
                                                                    i.key_pressed(Key::Escape)
                                                                }) {
                                                                    rename = Some((
                                                                        conv.id.clone(),
                                                                        None,
                                                                    ));
                                                                } else if r.lost_focus() {
                                                                    rename = Some((
                                                                        conv.id.clone(),
                                                                        Some(
                                                                            text.trim().to_string(),
                                                                        ),
                                                                    ));
                                                                }
                                                            }
                                                            _ => {
                                                                ui.add(
                                                                    egui::Label::new(
                                                                        RichText::new(&conv.title)
                                                                            .size(SMALL)
                                                                            .color(color),
                                                                    )
                                                                    .truncate(),
                                                                );
                                                            }
                                                        }
                                                        let n = conv.message_count;
                                                        ui.label(
                                                            RichText::new(format!(
                                                                "{n} message{}",
                                                                if n == 1 { "" } else { "s" }
                                                            ))
                                                            .size(10.0)
                                                            .color(TEXT_MUTED),
                                                        );
                                                    });
                                                    ui.with_layout(
                                                        Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            if widgets::icon_button(
                                                                ui,
                                                                icon::TRASH,
                                                                "Delete",
                                                            )
                                                            .clicked()
                                                            {
                                                                delete = Some(conv.id.clone());
                                                            }
                                                            if widgets::icon_button(
                                                                ui,
                                                                icon::PENCIL_SIMPLE,
                                                                "Rename",
                                                            )
                                                            .clicked()
                                                            {
                                                                self.renaming = Some((
                                                                    conv.id.clone(),
                                                                    conv.title.clone(),
                                                                ));
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                        let r = ui.interact(
                                            row.response.rect,
                                            ui.id().with("row"),
                                            egui::Sense::click(),
                                        );
                                        if r.clicked() && !streaming {
                                            select = Some(conv.id.clone());
                                        }
                                        if streaming {
                                            r.on_hover_text("Wait for the reply to finish");
                                        }
                                    });
                                }
                            });
                    });
            });

        if new_chat {
            self.new_chat(env);
        }
        if let Some(id) = select {
            self.select_conversation(env, id);
        }
        if let Some((id, title)) = rename {
            self.renaming = None;
            if let Some(title) = title.filter(|t| !t.is_empty()) {
                self.conv_ops.push(
                    env.ctx
                        .blocking(move |s| backend::chat::rename_conversation(s, &id, &title)),
                );
            }
        }
        if let Some(id) = delete {
            if self.active_conv.as_deref() == Some(id.as_str()) {
                self.active_conv = None;
                self.messages.clear();
            }
            self.conv_ops.push(
                env.ctx
                    .blocking(move |s| backend::chat::delete_conversation(s, &id)),
            );
        }
    }
}

fn active_model_name(env: &Env) -> Option<String> {
    let handle = env.shared.active_handle?;
    env.shared
        .loaded
        .iter()
        .find(|m| m.handle_id == handle)
        .map(|m| model_display_name(&m.model_path))
}

async fn blocking<T: Send + 'static>(
    state: &Arc<AppState>,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| format!("Background task failed: {e}"))?
}

fn avatar(ui: &mut Ui, role: Role) {
    let (fill, color, glyph) = match role {
        Role::User => (tint(PAW_500, 0.15), PAW_400, icon::USER),
        _ => (SURFACE_OVERLAY, TEXT_MUTED, icon::ROBOT),
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 12.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(12.0),
        color,
    );
}

#[allow(clippy::too_many_arguments)]
fn message_row(
    ui: &mut Ui,
    msg: &Msg,
    model: &str,
    streaming: bool,
    can_edit: bool,
    can_regen: bool,
    index: usize,
    md_cache: &mut CommonMarkCache,
    env: &mut Env,
) -> Option<MsgAction> {
    let mut action = None;
    ui.horizontal_top(|ui| {
        avatar(ui, msg.role);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                let who = match msg.role {
                    Role::User => "You",
                    Role::System => "System",
                    Role::Assistant => model,
                };
                ui.label(RichText::new(who).size(TINY).strong().color(TEXT_MUTED));
                if !msg.content.is_empty() && !streaming {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if can_regen
                            && widgets::icon_button(ui, icon::ARROWS_CLOCKWISE, "Regenerate")
                                .clicked()
                        {
                            action = Some(MsgAction::Regenerate);
                        }
                        if can_edit
                            && widgets::icon_button(ui, icon::PENCIL_SIMPLE, "Edit and resend")
                                .clicked()
                        {
                            action = Some(MsgAction::Edit(index));
                        }
                        if widgets::icon_button(ui, icon::COPY, "Copy").clicked() {
                            ui.ctx().copy_text(msg.content.clone());
                            env.toasts.success("Copied to clipboard.");
                        }
                    });
                }
            });

            if msg.error {
                ui.add(egui::Label::new(RichText::new(&msg.content).color(DANGER)).wrap());
            } else if msg.role == Role::User {
                ui.add(egui::Label::new(widgets::text(&msg.content)).wrap());
            } else if msg.content.is_empty() && streaming {
                thinking_dots(ui);
            } else {
                CommonMarkViewer::new().show(ui, md_cache, &msg.content);
                if streaming {
                    ui.label(RichText::new("▍").color(PAW_500));
                }
            }
        });
    });
    action
}

/// Three pulsing dots while the model has not produced anything yet.
fn thinking_dots(ui: &mut Ui) {
    let t = ui.input(|i| i.time) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 16.0), egui::Sense::hover());
    for i in 0..3 {
        let phase = (t * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5;
        let center = egui::pos2(rect.left() + 6.0 + i as f32 * 12.0, rect.center().y);
        ui.painter()
            .circle_filled(center, 3.0, tint(PAW_500, 0.3 + 0.7 * phase));
    }
    ui.ctx().request_repaint();
}

/// Inline editor for a user message. `Some(true)` = submit, `Some(false)` = cancel.
fn edit_box(ui: &mut Ui, text: &mut String) -> Option<bool> {
    let mut out = None;
    ui.horizontal_top(|ui| {
        avatar(ui, Role::User);
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .desired_rows(3)
                    .desired_width(ui.available_width()),
            );
            ui.horizontal(|ui| {
                if Btn::primary("", "Save & Submit").small().show(ui).clicked() {
                    out = Some(true);
                }
                if Btn::ghost("", "Cancel").small().show(ui).clicked() {
                    out = Some(false);
                }
            });
        });
    });
    out
}
