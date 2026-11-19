use crate::{roblox, store::Account};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    platform::{
        run_return::EventLoopExtRunReturn,
        windows::{EventLoopBuilderExtWindows, WindowBuilderExtWindows},
    },
    window::{Window, WindowBuilder, WindowId},
};
use wry::{WebContext, WebView, WebViewBuilder};

pub enum Notice {
    Status(String),
    Login(Result<Account, String>),
    Probed(Result<(), String>),
}
enum Command {
    Open(Option<String>),
    Probe,
    Validated(WindowId, Result<Account, String>),
    Shutdown,
}
pub struct Service {
    proxy: EventLoopProxy<Command>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Service {
    pub fn start(sender: mpsc::Sender<Notice>, ctx: eframe::egui::Context) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let mut event_loop = EventLoopBuilder::<Command>::with_user_event()
                .with_any_thread(true)
                .build();
            let proxy = event_loop.create_proxy();
            if ready_tx.send(proxy.clone()).is_err() {
                return;
            }
            let notify = |notice| {
                let _ = sender.send(notice);
                ctx.request_repaint();
            };
            let mut browsers: Vec<Browser> = Vec::new();
            let mut validating = None;
            event_loop.run_return(|event, target, control| {
                match event {
                    Event::UserEvent(Command::Probe) => {
                        // Exercise WebView2 on its own UI thread without contacting Roblox.
                        let result = browser(target, None, true).map(drop);
                        notify(Notice::Probed(result));
                    }
                    Event::UserEvent(Command::Shutdown) => {
                        browsers.clear();
                        *control = ControlFlow::Exit;
                        return;
                    }
                    Event::UserEvent(Command::Open(cookie)) => {
                        let login = cookie.is_none();
                        if login && browsers.iter().any(|b| b.login) {
                            notify(Notice::Status("A sign-in window is already open.".into()));
                        } else {
                            match browser(target, cookie.as_deref(), false) {
                                Ok(browser) => {
                                    browsers.push(browser);
                                    notify(Notice::Status(
                                        if login {
                                            "Sign in on Roblox's page. Close that window to cancel."
                                        } else {
                                            "Account browser opened."
                                        }
                                        .into(),
                                    ));
                                }
                                Err(error) => notify(if login {
                                    Notice::Login(Err(error))
                                } else {
                                    Notice::Status(error)
                                }),
                            }
                        }
                    }
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::Resized(size),
                        ..
                    } => {
                        if size.width > 0 && size.height > 0 {
                            if let Some(view) = browsers
                                .iter()
                                .find(|b| b.window.id() == window_id)
                                .and_then(|b| b.view.as_ref())
                            {
                                if view
                                    .set_bounds(wry::Rect {
                                        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                                        size: wry::dpi::PhysicalSize::new(size.width, size.height)
                                            .into(),
                                    })
                                    .is_err()
                                {
                                    notify(Notice::Status(
                                        "Could not resize the account browser.".into(),
                                    ));
                                }
                            }
                        }
                    }
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::CloseRequested,
                        ..
                    } => {
                        if browsers
                            .iter()
                            .any(|b| b.window.id() == window_id && b.login)
                        {
                            notify(Notice::Login(Err(
                                "Login canceled. No account was saved.".into()
                            )));
                        }
                        browsers.retain(|b| b.window.id() != window_id);
                    }
                    Event::UserEvent(Command::Validated(id, result)) => {
                        validating = None;
                        if let Some(index) = browsers.iter().position(|b| b.window.id() == id) {
                            match result {
                                Ok(account) => {
                                    browsers.remove(index);
                                    notify(Notice::Login(Ok(account)));
                                }
                                Err(error) => {
                                    browsers[index].next_poll =
                                        Instant::now() + Duration::from_secs(15);
                                    notify(Notice::Status(format!(
                                        "{error} Login will retry in 15 seconds."
                                    )));
                                }
                            }
                        }
                    }
                    Event::MainEventsCleared if validating.is_none() => {
                        for browser in browsers.iter_mut().filter(|b| b.login) {
                            if browser.started.elapsed() >= Duration::from_secs(600) {
                                browser.login = false;
                                browser
                                    .window
                                    .set_title("Login timed out - close and sign in again");
                                notify(Notice::Login(Err(
                                    "Login timed out. Close the browser and try again.".into(),
                                )));
                                continue;
                            }
                            if Instant::now() < browser.next_poll {
                                continue;
                            }
                            browser.next_poll = Instant::now() + Duration::from_secs(2);
                            if let Ok(cookies) = browser
                                .view
                                .as_ref()
                                .unwrap()
                                .cookies_for_url("https://www.roblox.com/")
                            {
                                if let Some(cookie) = cookies
                                    .iter()
                                    .find(|c| c.name() == ".ROBLOSECURITY" && !c.value().is_empty())
                                {
                                    let cookie = cookie.value().to_owned();
                                    let id = browser.window.id();
                                    let proxy = proxy.clone();
                                    validating = Some(id);
                                    notify(Notice::Status("Validating login...".into()));
                                    thread::spawn(move || {
                                        let _ = proxy.send_event(Command::Validated(
                                            id,
                                            roblox::validate(cookie),
                                        ));
                                    });
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                *control = if validating.is_none() {
                    browsers
                        .iter()
                        .filter(|b| b.login)
                        .map(|b| b.next_poll)
                        .min()
                        .map(ControlFlow::WaitUntil)
                        .unwrap_or(ControlFlow::Wait)
                } else {
                    ControlFlow::Wait
                };
            });
        });
        let proxy = ready_rx
            .recv()
            .map_err(|_| "Could not start the browser service.")?;
        Ok(Self {
            proxy,
            thread: Some(thread),
        })
    }
    pub fn open(&self, cookie: Option<String>) -> Result<(), String> {
        self.proxy
            .send_event(Command::Open(cookie))
            .map_err(|_| "The browser service has stopped. Restart the app.".into())
    }
    pub fn probe(&self) -> Result<(), String> {
        self.proxy
            .send_event(Command::Probe)
            .map_err(|_| "Browser service stopped.".into())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.proxy.send_event(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct Browser {
    view: Option<WebView>,
    context: Option<WebContext>,
    profile: Option<tempfile::TempDir>,
    window: Window,
    login: bool,
    started: Instant,
    next_poll: Instant,
}
impl Drop for Browser {
    fn drop(&mut self) {
        drop(self.view.take());
        drop(self.context.take());
        if let Some(profile) = self.profile.take() {
            let _ = profile.close();
        }
    }
}
fn browser(
    target: &EventLoopWindowTarget<Command>,
    cookie: Option<&str>,
    offline: bool,
) -> Result<Browser, String> {
    let window = WindowBuilder::new()
        // Tao and eframe's winit share a default class name but use different
        // window procedures. A unique class prevents dispatch into the wrong one.
        .with_window_classname("PRAC Roblox Browser")
        .with_visible(!offline)
        .with_title(if cookie.is_some() {
            "Roblox account browser"
        } else {
            "Sign in to Roblox"
        })
        .with_inner_size(LogicalSize::new(1100., 800.))
        .build(target)
        .map_err(|_| "Could not create a browser window.")?;
    crate::platform::set_window_icons(&window);
    let profile = tempfile::Builder::new()
        .prefix("prac-browser-")
        .tempdir()
        .map_err(|_| "Could not create an isolated browser profile.")?;
    let mut context = WebContext::new(Some(profile.path().to_path_buf()));
    let view = WebViewBuilder::new_with_web_context(&mut context)
        .with_incognito(true)
        .with_devtools(false)
        .with_url("about:blank")
        .with_navigation_handler(|url| url == "about:blank" || url.starts_with("https://"))
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .build(&window)
        .map_err(|_| "Could not initialize WebView2. Check the runtime installation.")?;
    if let Some(value) = cookie {
        let cookie = wry::cookie::Cookie::build((".ROBLOSECURITY", value))
            .domain(".roblox.com")
            .path("/")
            .secure(true)
            .http_only(true)
            .build();
        view.set_cookie(&cookie)
            .map_err(|_| "Could not initialize the account browser session.")?;
    }
    view.load_url(if offline {
        "about:blank"
    } else if cookie.is_some() {
        "https://www.roblox.com/home"
    } else {
        "https://www.roblox.com/login"
    })
    .map_err(|_| "Could not navigate to Roblox.")?;
    Ok(Browser {
        view: Some(view),
        context: Some(context),
        profile: Some(profile),
        window,
        login: cookie.is_none(),
        started: Instant::now(),
        next_poll: Instant::now() + Duration::from_secs(2),
    })
}
