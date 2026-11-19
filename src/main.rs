#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod background;
mod browser;
mod games;
mod platform;
mod roblox;
mod settings;
mod store;
use eframe::egui;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use store::Account;

type WorkResult = Result<(Option<Vec<Account>>, String), String>;
const ROBLOX_WARNING: &str = "Multiple Roblox instances are unavailable. Close all Roblox instances (check the system tray), then restart this app to run multiple instances.";
fn join_batch(
    accounts: Vec<Account>,
    target: roblox::Target,
    mut launch: impl FnMut(Account, roblox::Target) -> Result<(), String>,
    mut progress: impl FnMut(String),
) -> String {
    let total = accounts.len();
    let mut succeeded = 0;
    let mut failures = Vec::new();
    for (index, account) in accounts.into_iter().enumerate() {
        let name = account.username.clone();
        progress(format!("Launching {}/{}: {name}...", index + 1, total));
        match launch(account, target.clone()) {
            Ok(()) => succeeded += 1,
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    let mut message = format!("Launch sent for {succeeded}/{total} accounts.");
    if !failures.is_empty() {
        message.push_str("\nFailed:\n");
        message.push_str(&failures.join("\n"));
    }
    message
}
enum ResultEvent {
    Progress(String),
    LoadedGames(Result<Vec<games::SavedGame>, String>),
    SavedGames(Result<(Vec<games::SavedGame>, String), String>),
    Loaded(Result<Vec<Account>, String>),
    Worked(WorkResult),
}
struct Controller {
    accounts: Vec<Account>,
    selected: BTreeSet<usize>,
    selection_anchor: Option<usize>,
    mixed_descriptions: bool,
    description_edited: bool,
    description: String,
    place: String,
    saved_games: Vec<games::SavedGame>,
    games_error: Option<String>,
    username: String,
    message: String,
    busy: bool,
    blocked: bool,
    login_pending: bool,
    show_settings: bool,
    settings: settings::Settings,
    cpu_cap_input: u32,
    roblox_mutex: Option<platform::MutexGuard>,
    roblox_warning: bool,
    path: PathBuf,
    sender: mpsc::Sender<ResultEvent>,
    receiver: mpsc::Receiver<ResultEvent>,
    browser_sender: mpsc::Sender<browser::Notice>,
    browser_receiver: mpsc::Receiver<browser::Notice>,
    browsers: Option<browser::Service>,
    smoke: Option<(u8, Instant)>,
}
impl Controller {
    fn new(ctx: &egui::Context, path: PathBuf, smoke: bool) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (browser_sender, browser_receiver) = mpsc::channel();
        let load_sender = sender.clone();
        let load_path = path.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = load_sender.send(ResultEvent::LoadedGames(games::load(
                &load_path.with_file_name("games.json"),
            )));
            let _ = load_sender.send(ResultEvent::Loaded(store::load(&load_path)));
            repaint.request_repaint();
        });
        Self {
            accounts: Vec::new(),
            selected: BTreeSet::new(),
            selection_anchor: None,
            mixed_descriptions: false,
            description_edited: false,
            description: String::new(),
            place: String::new(),
            saved_games: Vec::new(),
            games_error: None,
            username: String::new(),
            message: "Loading accounts...".into(),
            busy: true,
            blocked: false,
            login_pending: false,
            show_settings: false,
            settings: settings::Settings::default(),
            cpu_cap_input: settings::Settings::default().background_cpu_percent,
            roblox_mutex: None,
            roblox_warning: false,
            path,
            sender,
            receiver,
            browser_sender,
            browser_receiver,
            browsers: None,
            smoke: smoke.then(|| (0, Instant::now())),
        }
    }
    fn select(&mut self, index: usize) {
        self.select_click(index, false, false);
    }
    fn select_click(&mut self, index: usize, ctrl: bool, shift: bool) {
        if index >= self.accounts.len() {
            return;
        }
        if shift {
            let anchor = self.selection_anchor.unwrap_or(index);
            if !ctrl {
                self.selected.clear();
            }
            self.selected.extend(anchor.min(index)..=anchor.max(index));
            self.selection_anchor = Some(anchor);
        } else {
            if ctrl {
                if !self.selected.remove(&index) {
                    self.selected.insert(index);
                }
            } else {
                self.selected.clear();
                self.selected.insert(index);
            }
            self.selection_anchor = Some(index);
        }
        self.refresh_description();
    }
    fn refresh_description(&mut self) {
        let first = self
            .selected
            .first()
            .map(|&i| self.accounts[i].description.as_str())
            .unwrap_or("");
        self.mixed_descriptions = self
            .selected
            .iter()
            .any(|&i| self.accounts[i].description != first);
        if self.selected.len() > 1 {
            return;
        }
        self.description = first.to_owned();
        self.description_edited = false;
    }
    fn replace_accounts(&mut self, accounts: Vec<Account>) {
        let selected: Vec<_> = self
            .selected
            .iter()
            .filter_map(|&i| self.accounts.get(i))
            .map(|a| a.username.clone())
            .collect();
        let anchor = self
            .selection_anchor
            .and_then(|i| self.accounts.get(i))
            .map(|a| a.username.clone());
        self.accounts = accounts;
        self.selected = self
            .accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                selected
                    .iter()
                    .any(|name| a.username.eq_ignore_ascii_case(name))
            })
            .map(|(i, _)| i)
            .collect();
        self.selection_anchor = anchor.and_then(|name| {
            self.accounts
                .iter()
                .position(|a| a.username.eq_ignore_ascii_case(&name))
        });
        self.refresh_description();
    }
    fn work(
        &mut self,
        ctx: &egui::Context,
        message: &str,
        task: impl FnOnce() -> WorkResult + Send + 'static,
    ) {
        self.busy = true;
        self.message = message.into();
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(ResultEvent::Worked(task()));
            repaint.request_repaint();
        });
    }
    fn save(&mut self, ctx: &egui::Context, accounts: Vec<Account>, message: impl Into<String>) {
        let message = message.into();
        let path = self.path.clone();
        self.work(ctx, "Saving encrypted accounts...", move || {
            store::save(&path, &accounts)?;
            Ok((Some(accounts), message))
        });
    }
    fn save_description(&mut self, ctx: &egui::Context) {
        if self.selected.is_empty() || (self.mixed_descriptions && !self.description_edited) {
            return;
        }
        let mut accounts = self.accounts.clone();
        for &index in &self.selected {
            accounts[index].description = self.description.clone();
        }
        self.save(
            ctx,
            accounts,
            format!("Description saved for {} account(s).", self.selected.len()),
        );
    }
    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            if let ResultEvent::LoadedGames(result) = event {
                match result {
                    Ok(games) => self.saved_games = games,
                    Err(error) => self.games_error = Some(error),
                }
                continue;
            }
            if let ResultEvent::Progress(message) = event {
                self.message = message;
                continue;
            }
            self.busy = false;
            match event {
                ResultEvent::Progress(_) => unreachable!(),
                ResultEvent::LoadedGames(_) => unreachable!(),
                ResultEvent::SavedGames(result) => match result {
                    Ok((games, message)) => {
                        self.saved_games = games;
                        self.message = message;
                    }
                    Err(error) => self.message = error,
                },
                ResultEvent::Loaded(Ok(accounts)) => {
                    self.replace_accounts(accounts);
                    self.message = "Ready. Select an account or add one.".into();
                }
                ResultEvent::Loaded(Err(error)) => {
                    self.blocked = true;
                    self.message = error;
                }
                ResultEvent::Worked(Ok((accounts, message))) => {
                    if let Some(accounts) = accounts {
                        self.replace_accounts(accounts);
                    }
                    self.message = message;
                }
                ResultEvent::Worked(Err(error)) => self.message = error,
            }
        }
        while let Ok(event) = self.browser_receiver.try_recv() {
            match event {
                browser::Notice::Probed(result) => {
                    assert!(self.smoke.is_some());
                    assert!(
                        result.is_ok(),
                        "Browser smoke test failed: {}",
                        result.err().unwrap_or_default()
                    );
                    self.browsers = None;
                    self.smoke.as_mut().unwrap().0 = 4;
                }
                browser::Notice::Status(message) => self.message = message,
                browser::Notice::Login(result) => {
                    self.login_pending = false;
                    match result {
                        Ok(mut account) => {
                            let mut accounts = self.accounts.clone();
                            if let Some(old) = accounts
                                .iter_mut()
                                .find(|a| a.username.eq_ignore_ascii_case(&account.username))
                            {
                                account.description = old.description.clone();
                                account.background = old.background;
                                *old = account;
                            } else {
                                accounts.push(account);
                            }
                            self.save(ctx, accounts, "Account saved. Ready to play.");
                        }
                        Err(error) => self.message = error,
                    }
                }
            }
        }
    }
    fn open_browser(&mut self, ctx: &egui::Context, login: bool) {
        if !login && self.selected.len() != 1 {
            self.message = "Select one account to open its browser.".into();
            return;
        }
        let result = (|| {
            if self.browsers.is_none() {
                self.browsers = Some(browser::Service::start(
                    self.browser_sender.clone(),
                    ctx.clone(),
                )?);
            }
            let cookie = if login {
                None
            } else {
                Some(
                    self.selected
                        .first()
                        .and_then(|&i| self.accounts.get(i))
                        .ok_or("Select an account first.")?
                        .cookie
                        .clone(),
                )
            };
            self.browsers.as_ref().unwrap().open(cookie)
        })();
        match result {
            Ok(()) => {
                self.login_pending = login;
                self.message = "Opening browser...".into();
            }
            Err(error) => self.message = error,
        }
    }
    fn import(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        let mut accounts = self.accounts.clone();
        self.work(ctx, "Choose a CSV file to import...", move || {
            let Some(source) = platform::pick_csv()? else {
                return Ok((None, "Import cancelled.".into()));
            };
            let file =
                std::fs::File::open(source).map_err(|_| "Could not open the selected CSV file.")?;
            let count = store::import_reader(file, &mut accounts)?;
            store::save(&path, &accounts)?;
            Ok((
                Some(accounts),
                format!("Imported {count} accounts. Existing usernames skipped."),
            ))
        });
    }
    fn join(&mut self, ctx: &egui::Context, follow: bool) {
        let accounts: Vec<_> = self
            .selected
            .iter()
            .map(|&i| self.accounts[i].clone())
            .collect();
        if accounts.is_empty() {
            return;
        }
        let target = if follow {
            let name = self.username.trim();
            if name.is_empty() || name.len() > 100 {
                self.message = "Enter a player username (up to 100 bytes).".into();
                return;
            }
            roblox::Target::User(name.into())
        } else {
            let Some(id) = self.place.trim().parse::<u64>().ok().filter(|id| *id > 0) else {
                self.message = "Enter a positive numeric place ID.".into();
                return;
            };
            roblox::Target::Place(id)
        };
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        let percent = self.settings.background_cpu_percent;
        self.work(ctx, "Requesting Roblox launches...", move || {
            let message = join_batch(
                accounts,
                target,
                |account, target| roblox::join(account, target, percent),
                |message| {
                    let _ = sender.send(ResultEvent::Progress(message));
                    repaint.request_repaint();
                },
            );
            Ok((None, message))
        });
    }
    fn accounts_ui(&mut self, ui: &mut egui::Ui, window_bottom: f32) {
        ui.heading("Accounts");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add Account/Sign Back In").clicked() {
                self.open_browser(ui.ctx(), true);
            }
        });
        ui.add_space(12.0);
        if self.accounts.is_empty() {
            ui.label("No accounts yet. Add an account or import your existing CSV in Settings.");
        }
        let mut selected = None;
        // Reserve room for the frame, selection count, and session note.
        let footer_height = 2.0 * egui::TextStyle::Small.resolve(ui.style()).size
            + 3.0 * ui.spacing().item_spacing.y
            + 22.0;
        let list_height = (window_bottom - ui.cursor().top() - footer_height).max(24.0);
        let accounts_frame = egui::Frame::group(ui.style())
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(4.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("accounts")
                    .max_height(list_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (index, account) in self.accounts.iter().enumerate() {
                            // Keep multiline notes in the editor; list entries stay one line.
                            let note = account
                                .description
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            let mode = if account.background {
                                " (background)"
                            } else {
                                ""
                            };
                            let title = format!("{}{mode}  |  {note}", account.username);
                            let row =
                                egui::Button::selectable(self.selected.contains(&index), title)
                                    .truncate()
                                    .corner_radius(0.0)
                                    .min_size(egui::vec2(ui.available_width(), 24.0));
                            if ui.add(row).on_hover_text(&account.description).clicked() {
                                selected = Some(index);
                            }
                        }
                    });
            });
        // Keep scrolling over the list from moving the outer window at either edge.
        if ui.rect_contains_pointer(accounts_frame.response.rect) {
            ui.input_mut(|input| input.smooth_scroll_delta.y = 0.0);
        }
        if let Some(index) = selected {
            let modifiers = ui.input(|i| i.modifiers);
            self.select_click(index, modifiers.ctrl || modifiers.command, modifiers.shift);
        }
        ui.small(format!("{} selected", self.selected.len()));
        ui.add_space(12.0);
        ui.small("Sessions are encrypted for your Windows user.");
    }
    fn save_game(&mut self, ctx: &egui::Context, remove: bool) {
        if self.games_error.is_some() {
            return;
        }
        let Some(id) = self.place.trim().parse::<u64>().ok().filter(|id| *id > 0) else {
            self.message = "Enter a positive numeric place ID.".into();
            return;
        };
        if !remove && self.saved_games.iter().any(|game| game.place_id == id) {
            self.message = "This game is already saved.".into();
            return;
        }
        let mut games = self.saved_games.clone();
        let path = self.path.with_file_name("games.json");
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        self.busy = true;
        self.message = if remove {
            "Removing saved game..."
        } else {
            "Looking up game title and saving..."
        }
        .into();
        std::thread::spawn(move || {
            let result = (|| {
                let message = if remove {
                    games.retain(|game| game.place_id != id);
                    "Saved game removed.".to_owned()
                } else {
                    let title = roblox::game_title(id)?;
                    let message = format!("Saved {title}.");
                    games.push(games::SavedGame {
                        place_id: id,
                        title,
                    });
                    message
                };
                games::save(&path, &games)?;
                Ok((games, message))
            })();
            let _ = sender.send(ResultEvent::SavedGames(result));
            repaint.request_repaint();
        });
    }
    fn games_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Place ID");
        ui.add(
            egui::TextEdit::singleline(&mut self.place)
                .desired_width(f32::INFINITY)
                .hint_text("Place ID to join"),
        );
        if let Some(error) = &self.games_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.horizontal_wrapped(|ui| {
            let count = self.selected.len();
            if ui
                .add_enabled(
                    count > 0,
                    egui::Button::new(if count > 1 {
                        format!("Join {count} accounts by place ID")
                    } else {
                        "Join by place ID".into()
                    }),
                )
                .clicked()
            {
                self.join(ui.ctx(), false);
            }
            if ui
                .add_enabled(self.games_error.is_none(), egui::Button::new("Save game"))
                .clicked()
            {
                self.save_game(ui.ctx(), false);
            }
            let saved = self
                .saved_games
                .iter()
                .any(|game| Some(game.place_id) == self.place.trim().parse::<u64>().ok());
            if ui
                .add_enabled(
                    saved && self.games_error.is_none(),
                    egui::Button::new("Remove saved game"),
                )
                .clicked()
            {
                self.save_game(ui.ctx(), true);
            }
        });
        if !self.saved_games.is_empty() {
            ui.label("Saved Games");
            let games_frame = egui::Frame::group(ui.style())
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(4.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("saved_games")
                        .max_height(3.0 * 24.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for game in &self.saved_games {
                                let label = format!("{} ({})", game.title, game.place_id);
                                if ui
                                    .add(
                                        egui::Button::selectable(
                                            self.place.trim() == game.place_id.to_string(),
                                            &label,
                                        )
                                        .truncate()
                                        .min_size(egui::vec2(ui.available_width(), 24.0)),
                                    )
                                    .on_hover_text(&label)
                                    .clicked()
                                {
                                    self.place = game.place_id.to_string();
                                }
                            }
                        });
                });
            // Consume wheel input left over at the list edges before the outer area sees it.
            if ui.rect_contains_pointer(games_frame.response.rect) {
                ui.input_mut(|input| input.smooth_scroll_delta.y = 0.0);
            }
        }
    }
    fn detail_ui(&mut self, ui: &mut egui::Ui) {
        let count = self.selected.len();
        let title = match count {
            0 => "Select an account".to_owned(),
            1 => self.accounts[*self.selected.first().unwrap()]
                .username
                .clone(),
            _ => format!("{count} accounts selected"),
        };
        ui.heading(title);
        ui.add_space(8.0);
        self.games_ui(ui);
        ui.add_space(8.0);
        ui.add_enabled_ui(count > 0, |ui| {
            let marked = self.selected.iter().filter(|&&i| self.accounts[i].background).count();
            let mut enabled = count > 0 && marked == count;
            if ui.add(egui::Checkbox::new(&mut enabled, "Launch as background")
                .indeterminate(marked > 0 && marked < count))
                .on_hover_text(format!("Background clients share {}% of total CPU. Applies on next launch.", self.settings.background_cpu_percent))
                .changed() {
                let mut accounts = self.accounts.clone();
                for &i in &self.selected {
                    accounts[i].background = enabled;
                }
                self.save(ui.ctx(), accounts, "Launch preference saved.");
            }
            ui.label("Player username");
            ui.add(
                egui::TextEdit::singleline(&mut self.username)
                    .desired_width(f32::INFINITY)
                    .char_limit(100)
                    .hint_text("Username to follow"),
            );
            if ui.button(if count > 1 { format!("Join {count} accounts by player username") } else { "Join by player username".into() }).clicked() {
                self.join(ui.ctx(), true);
            }
            ui.add_space(12.0);
            ui.label("Description");
            if ui.add(
                egui::TextEdit::multiline(&mut self.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(5)
                    .char_limit(10000)
                    .hint_text("Description for the selected accounts"),
            ).changed() { self.description_edited = true; }
            if ui.add_enabled(!self.mixed_descriptions || self.description_edited, egui::Button::new("Save description"))
                .on_hover_text("Save the description for every selected account.")
                .on_disabled_hover_text("Edit the description to replace the differing descriptions of the selected accounts.")
                .clicked() {
                self.save_description(ui.ctx());
            }
            ui.add_space(12.0);
            if ui.add_enabled(count == 1, egui::Button::new("Open account browser")).on_disabled_hover_text("Select one account to open its browser.").clicked() {
                self.open_browser(ui.ctx(), false);
            }
        });
    }
    fn apply_settings(
        &mut self,
        ctx: &egui::Context,
        next: settings::Settings,
    ) -> Result<(), String> {
        let new_mutex = if next.singleton && self.roblox_mutex.is_none() {
            match platform::MutexGuard::create_available("ROBLOX_singletonEvent") {
                Ok(guard) => Some(guard),
                Err(_) => {
                    self.roblox_warning = true;
                    None
                }
            }
        } else {
            None
        };
        let cap_changed = next.background_cpu_percent != self.settings.background_cpu_percent;
        if cap_changed {
            background::configure(next.background_cpu_percent)?;
        }
        if let Err(error) = next.save(&self.path.with_file_name("settings.json")) {
            if cap_changed {
                background::configure(self.settings.background_cpu_percent)?;
            }
            return Err(error);
        }
        if !next.singleton {
            self.roblox_mutex = None;
        } else if new_mutex.is_some() {
            self.roblox_mutex = new_mutex;
            self.roblox_warning = false;
        }
        ctx.set_visuals(if next.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        self.settings = next;
        Ok(())
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(8.0);
        let mut next = self.settings;
        let singleton_changed = ui
            .add_enabled(
                !self.busy && !self.login_pending,
                egui::Checkbox::new(
                    &mut next.singleton,
                    "Enable Multiple Instances",
                ),
            )
            .changed();
        ui.small("Enable before launching Roblox. Changes may require closing existing Roblox clients and restarting PRAC");
        ui.add_space(12.0);
        let theme_changed = ui.checkbox(&mut next.dark_mode, "Dark mode").changed();
        ui.small("Turn off for light mode.");
        ui.add_space(12.0);
        ui.label("Background CPU cap");
        ui.add(egui::Slider::new(&mut self.cpu_cap_input, 1..=100).suffix("%"));
        ui.small("All background accounts share this percentage of total CPU.");
        let apply_cap = ui
            .add_enabled(
                !self.busy && self.cpu_cap_input != next.background_cpu_percent,
                egui::Button::new("Apply CPU cap"),
            )
            .clicked();
        if apply_cap {
            next.background_cpu_percent = self.cpu_cap_input;
        }
        if singleton_changed || theme_changed || apply_cap {
            self.message = match self.apply_settings(ui.ctx(), next) {
                Ok(()) => "Settings saved.".into(),
                Err(error) => error,
            };
        }
        ui.add_space(16.0);
        ui.separator();
        if ui
            .add_enabled(
                !self.busy && !self.login_pending,
                egui::Button::new("Trim background RAM"),
            )
            .clicked()
        {
            self.work(ui.ctx(), "Trimming background RAM...", || {
                background::trim_ram().map(|message| (None, message))
            });
        }
        ui.small("One-time trim for running PRAC background clients. Active clients may briefly stutter as they reload memory.");
        ui.add_space(16.0);
        ui.heading("Import accounts");
        ui.label("Choose a CSV file to import into your encrypted account store. Existing usernames will be skipped.");
        let can_import = !self.busy && !self.blocked && !self.login_pending;
        if ui
            .add_enabled(can_import, egui::Button::new("Import old CSV"))
            .clicked()
        {
            self.import(ui.ctx());
        }
    }

    fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(32.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    if self.busy || self.login_pending {
                        ui.put(rect, egui::Spinner::new().size(18.0));
                    }
                    ui.add(egui::Label::new(&self.message).truncate())
                        .on_hover_text(&self.message);
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            egui::ScrollArea::vertical()
                .id_salt("main")
                .auto_shrink([false, false])
                .show_viewport(ui, |ui, viewport| {
                    // Use the unscrolled viewport bottom so outer scrolling cannot resize the list.
                    let window_bottom = ui.max_rect().top() + viewport.height();
                    if self.roblox_warning {
                        ui.colored_label(ui.visuals().warn_fg_color, ROBLOX_WARNING);
                        ui.separator();
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.heading("Personal Roblox Account Controller");
                        if ui
                            .selectable_label(!self.show_settings, "Accounts")
                            .clicked()
                        {
                            self.show_settings = false;
                        }
                        if ui
                            .selectable_label(self.show_settings, "Settings")
                            .clicked()
                        {
                            self.show_settings = true;
                        }
                    });
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    if self.show_settings {
                        self.settings_ui(ui);
                    } else {
                        ui.add_enabled_ui(
                            !self.busy && !self.blocked && !self.login_pending,
                            |ui| {
                                if ui.available_width() >= 720.0 {
                                    ui.columns(2, |columns| {
                                        self.accounts_ui(&mut columns[0], window_bottom);
                                        self.detail_ui(&mut columns[1]);
                                    });
                                } else {
                                    self.accounts_ui(ui, window_bottom);
                                    ui.add_space(16.0);
                                    ui.separator();
                                    self.detail_ui(ui);
                                }
                            },
                        );
                    }
                });
        });
    }
    fn smoke_step(&mut self, ctx: &egui::Context) {
        let Some((stage, started)) = self.smoke else {
            return;
        };
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "egui smoke test timed out"
        );
        match stage {
            0 if !self.busy => {
                println!("Smoke: egui loaded.");
                assert!(!self.blocked);
                assert_eq!(self.accounts.len(), 1);
                self.select(0);
                assert_eq!(self.description, "<script>unsafe</script>\nTest note");
                self.description = "Saved by egui smoke test".into();
                let mut accounts = self.accounts.clone();
                accounts[0].description = self.description.clone();
                self.save(ctx, accounts, "Smoke description saved.");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(360.0, 480.0)));
                self.smoke.as_mut().unwrap().0 = 1;
            }
            1 if !self.busy && ctx.content_rect().width() < 400.0 => {
                println!("Smoke: narrow window and encrypted save checked.");
                assert_eq!(
                    store::load(&self.path).unwrap()[0].description,
                    "Saved by egui smoke test"
                );
                ctx.set_visuals(egui::Visuals::light());
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1050.0, 780.0)));
                self.smoke.as_mut().unwrap().0 = 2;
            }
            2 if ctx.content_rect().width() > 900.0 => {
                println!("Smoke: wide window checked; starting offline browser.");
                assert!(!ctx.style().visuals.dark_mode);
                let service = browser::Service::start(self.browser_sender.clone(), ctx.clone())
                    .expect("Browser thread");
                service.probe().expect("Browser probe");
                self.browsers = Some(service);
                self.smoke.as_mut().unwrap().0 = 3;
            }
            4 => {
                println!(
                    "Synthetic egui UI smoke test passed (load, select, save, resize, theme, offline browser lifecycle)."
                );
                self.smoke.as_mut().unwrap().0 = 5;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
impl eframe::App for Controller {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
        if self.busy && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.message = "Please wait for the current operation to finish before closing.".into();
        }
        self.draw(ctx);
        self.smoke_step(ctx);
    }
}
fn run() -> Result<(), String> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let _app_mutex = platform::MutexGuard::create(
        if smoke {
            "Local\\PersonalRobloxAccountController_Smoke"
        } else {
            "Local\\PersonalRobloxAccountController_Rust"
        },
        true,
    )?;
    let smoke_profile = if smoke {
        Some(tempfile::tempdir().map_err(|_| "Could not create test store.")?)
    } else {
        None
    };
    let path = if let Some(temp) = &smoke_profile {
        let path = temp.path().join("test.dpapi");
        store::save(
            &path,
            &[Account {
                username: "SyntheticUser".into(),
                cookie: "synthetic-cookie-never-sent".into(),
                description: "<script>unsafe</script>\nTest note".into(),
                background: false,
            }],
        )?;
        path
    } else {
        store::location()?
    };
    let settings = settings::Settings::load(&path.with_file_name("settings.json"))?;
    let (roblox_mutex, roblox_warning) = if settings.singleton && !smoke {
        match platform::MutexGuard::create_available("ROBLOX_singletonEvent") {
            Ok(guard) => (Some(guard), false),
            Err(_) => (None, true),
        }
    } else {
        (None, false)
    };
    let icon =
        image::load_from_memory_with_format(include_bytes!("../app.ico"), image::ImageFormat::Ico)
            .map_err(|error| format!("Could not load the application icon: {error}"))?
            .into_rgba8();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(egui::IconData {
                width: icon.width(),
                height: icon.height(),
                rgba: icon.into_raw(),
            })
            .with_inner_size([1050.0, 780.0])
            .with_min_inner_size([320.0, 240.0])
            .with_visible(!smoke),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Personal Roblox Account Controller",
        options,
        Box::new(move |cc| {
            // Set both Windows icon sizes even when the controller starts in the background.
            platform::set_window_icons(cc);
            if smoke {
                println!("Smoke: native renderer initialized.");
            }
            cc.egui_ctx.set_visuals(if settings.dark_mode {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let mut app = Controller::new(&cc.egui_ctx, path, smoke);
            app.settings = settings;
            app.cpu_cap_input = settings.background_cpu_percent;
            app.roblox_mutex = roblox_mutex;
            app.roblox_warning = roblox_warning;
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| format!("Could not run the desktop interface: {error}"))
}
fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, egui::Context, Controller) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.dpapi");
        store::save(
            &path,
            &[Account {
                username: "SyntheticUser".into(),
                cookie: "secret-must-never-be-painted".into(),
                description: "Long note ".repeat(100),
                background: false,
            }],
        )
        .unwrap();
        let ctx = egui::Context::default();
        let mut app = Controller::new(&ctx, path, false);
        let games_event = app.receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        let accounts_event = app.receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        app.sender.send(games_event).unwrap();
        app.sender.send(accounts_event).unwrap();
        app.receive(&ctx);
        (directory, ctx, app)
    }

    #[test]
    fn saved_game_controls_reject_invalid_duplicates_and_persist_removal() {
        let (_directory, ctx, mut app) = fixture();
        app.place = "not an ID".into();
        app.save_game(&ctx, false);
        assert!(!app.busy);
        assert!(app.message.contains("positive numeric"));
        app.saved_games = vec![games::SavedGame {
            place_id: 42,
            title: "Saved title".into(),
        }];
        games::save(&app.path.with_file_name("games.json"), &app.saved_games).unwrap();
        app.place = "42".into();
        app.save_game(&ctx, false);
        assert!(!app.busy);
        assert!(app.message.contains("already saved"));
        app.save_game(&ctx, true);
        let event = app.receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        app.sender.send(event).unwrap();
        app.receive(&ctx);
        assert!(!app.busy);
        assert!(app.saved_games.is_empty());
        assert!(games::load(&app.path.with_file_name("games.json"))
            .unwrap()
            .is_empty());
        app.saved_games.push(games::SavedGame {
            place_id: 42,
            title: "Cached title".into(),
        });
        app.sender
            .send(ResultEvent::SavedGames(Err("Lookup failed".into())))
            .unwrap();
        app.receive(&ctx);
        assert_eq!(app.saved_games[0].title, "Cached title");
        assert_eq!(app.message, "Lookup failed");
        app.games_error = Some("Invalid saved games file".into());
        app.save_game(&ctx, true);
        assert!(!app.busy);
        assert_eq!(app.saved_games.len(), 1);
    }

    #[test]
    fn selection_survives_account_reordering_and_clears_when_removed() {
        let (_directory, _ctx, mut app) = fixture();
        app.select(0);
        let original = app.accounts[0].clone();
        let mut other = original.clone();
        other.username = "Other".into();
        app.replace_accounts(vec![other.clone(), original.clone()]);
        assert_eq!(app.selected, BTreeSet::from([1]));
        assert_eq!(app.description, original.description);
        app.replace_accounts(vec![other]);
        assert!(app.selected.is_empty());
        assert!(app.description.is_empty());
    }

    #[test]
    fn corrupt_store_blocks_mutating_controls() {
        let (_directory, ctx, mut app) = fixture();
        app.sender
            .send(ResultEvent::Loaded(Err("Invalid store".into())))
            .unwrap();
        app.receive(&ctx);
        assert!(app.blocked);
        assert!(!app.busy);
        assert_eq!(app.message, "Invalid store");
    }

    fn populate(app: &mut Controller) {
        app.replace_accounts(
            (0..6)
                .map(|i| Account {
                    username: format!("User{i}"),
                    cookie: "synthetic-only".into(),
                    description: format!("Note {i}"),
                    background: false,
                })
                .collect(),
        );
    }

    #[test]
    fn ctrl_toggles_shift_ranges_and_plain_click_resets_selection() {
        let (_directory, _ctx, mut app) = fixture();
        populate(&mut app);
        app.select(1);
        app.select_click(4, true, false);
        assert_eq!(app.selected, BTreeSet::from([1, 4]));
        assert_eq!(app.description, "Note 1");
        app.select_click(4, true, false);
        assert_eq!(app.selected, BTreeSet::from([1]));
        app.select_click(2, false, true); // range from the last clicked anchor, in reverse
        assert_eq!(app.selected, BTreeSet::from([2, 3, 4]));
        assert_eq!(app.description, "Note 1");
        app.description = "Draft note".into();
        app.description_edited = true;
        app.select_click(5, false, true); // anchor remains 4
        assert_eq!(app.selected, BTreeSet::from([4, 5]));
        assert_eq!(app.description, "Draft note");
        assert!(app.description_edited);
        app.select_click(1, true, true); // add a range to the existing selection
        assert_eq!(app.selected, BTreeSet::from([1, 2, 3, 4, 5]));
        app.select(0);
        assert_eq!(app.selected, BTreeSet::from([0]));
        assert_eq!(app.description, "Note 0");
        app.select_click(0, true, false);
        assert!(app.selected.is_empty());
        assert!(app.description.is_empty());
    }

    #[test]
    fn batch_selection_and_anchor_follow_accounts_after_reload() {
        let (_directory, _ctx, mut app) = fixture();
        populate(&mut app);
        app.select(1);
        app.select_click(4, true, false);
        let mut reordered = app.accounts.clone();
        reordered.reverse();
        app.replace_accounts(reordered);
        assert_eq!(app.selected, BTreeSet::from([1, 4]));
        assert_eq!(app.selection_anchor, Some(1)); // User4
        app.select_click(0, false, true);
        assert_eq!(app.selected, BTreeSet::from([0, 1]));
    }

    #[test]
    fn batch_description_save_changes_only_selected_accounts_and_preserves_cookies() {
        let (_directory, ctx, mut app) = fixture();
        populate(&mut app);
        store::save(&app.path, &app.accounts).unwrap();
        app.select(1);
        app.select_click(3, true, false);
        assert!(app.mixed_descriptions);
        app.save_description(&ctx);
        assert!(!app.busy); // untouched mixed descriptions cannot be accidentally cleared
        assert_eq!(store::load(&app.path).unwrap()[1].description, "Note 1");
        for replacement in ["Shared\nmultiline note", ""] {
            app.description = replacement.into();
            app.description_edited = true;
            app.save_description(&ctx);
            assert!(app.busy);
            let event = app.receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            app.sender.send(event).unwrap();
            app.receive(&ctx);
            let saved = store::load(&app.path).unwrap();
            for (i, account) in saved.iter().enumerate() {
                assert_eq!(
                    account.description,
                    if i == 1 || i == 3 {
                        replacement.to_owned()
                    } else {
                        format!("Note {i}")
                    }
                );
                assert_eq!(account.cookie, "synthetic-only");
            }
            assert_eq!(app.selected, BTreeSet::from([1, 3]));
            assert!(!app.mixed_descriptions);
        }
    }

    #[test]
    fn batch_join_attempts_every_account_and_reports_partial_failure_for_both_targets() {
        let (_directory, _ctx, mut app) = fixture();
        populate(&mut app);
        for target in [
            roblox::Target::Place(42),
            roblox::Target::User("Friend".into()),
        ] {
            let accounts = vec![
                app.accounts[1].clone(),
                app.accounts[3].clone(),
                app.accounts[5].clone(),
            ];
            let mut attempted = Vec::new();
            let mut updates = Vec::new();
            let result = join_batch(
                accounts,
                target.clone(),
                |account, actual| {
                    match (&target, actual) {
                        (roblox::Target::Place(expected), roblox::Target::Place(actual)) => {
                            assert_eq!(*expected, actual)
                        }
                        (roblox::Target::User(expected), roblox::Target::User(actual)) => {
                            assert_eq!(*expected, actual)
                        }
                        _ => panic!("Batch target changed"),
                    }
                    attempted.push(account.username.clone());
                    if account.username == "User3" {
                        Err("Synthetic launch failure".into())
                    } else {
                        Ok(())
                    }
                },
                |message| updates.push(message),
            );
            assert_eq!(attempted, ["User1", "User3", "User5"]);
            assert_eq!(updates.len(), 3);
            assert!(result.contains("2/3"));
            assert!(result.contains("User3: Synthetic launch failure"));
            assert!(!result.contains("synthetic-only"));
        }
    }

    #[test]
    fn resized_layout_wraps_and_save_remains_reachable_without_painting_credentials() {
        for size in [
            egui::vec2(320.0, 240.0),
            egui::vec2(360.0, 480.0),
            egui::vec2(1050.0, 780.0),
        ] {
            let (_directory, ctx, mut app) = fixture();
            app.select(0);
            let mut saw_save = false;
            for frame in 0..25 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    time: Some(frame as f64 * 0.1),
                    events: if frame == 0 {
                        vec![]
                    } else {
                        vec![
                            egui::Event::PointerMoved(egui::pos2(size.x - 20.0, size.y / 2.0)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -160.0),
                                modifiers: egui::Modifiers::default(),
                            },
                        ]
                    },
                    ..Default::default()
                };
                let output = ctx.run(input, |ctx| app.draw(ctx));
                for clipped in output.shapes {
                    if let egui::epaint::Shape::Text(text) = clipped.shape {
                        let value = &text.galley.job.text;
                        assert!(!value.contains("secret-must-never-be-painted"));
                        let bounds = text.galley.rect.translate(text.pos.to_vec2());
                        if clipped.clip_rect.intersects(bounds) {
                            assert!(
                                bounds.right() <= size.x + 1.0,
                                "Text overflow at {size:?}: {value}"
                            );
                            if value == "Save description"
                                && clipped.clip_rect.contains_rect(bounds)
                            {
                                saw_save = true;
                            }
                        }
                    }
                }
            }
            assert!(
                saw_save,
                "Save description could not be reached at {size:?}"
            );
        }
    }
}
