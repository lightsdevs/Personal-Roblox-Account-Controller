mod platform;
mod roblox;
mod store;
use base64::Engine;
use serde::Deserialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use store::Account;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};
use wry::{WebContext, WebView, WebViewBuilder};

type WorkResult = Result<(Option<Vec<Account>>, String), String>;
enum AppEvent {
    Command(Command),
    Loaded(Result<Vec<Account>, String>),
    Worked(WorkResult),
    Validated(WindowId, Result<Account, String>),
}
#[derive(Deserialize)]
struct Command {
    op: String,
    index: Option<usize>,
    #[serde(default)]
    value: String,
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
    target: &EventLoopWindowTarget<AppEvent>,
    cookie: Option<&str>,
) -> Result<Browser, String> {
    let window = WindowBuilder::new()
        .with_title(if cookie.is_some() {
            "Roblox account browser"
        } else {
            "Sign in to Roblox"
        })
        .with_inner_size(LogicalSize::new(1100., 800.))
        .build(target)
        .map_err(|_| "Could not create a browser window.")?;
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
    view.load_url(if cookie.is_some() {
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
fn render(view: &WebView, accounts: Option<&[Account]>, busy: bool, blocked: bool, message: &str) {
    // Explicit allowlist: never serialize Account (which includes its cookie) to the UI.
    let public = accounts.map(|items| {
        items
            .iter()
            .map(|a| serde_json::json!({"username": a.username, "description": a.description}))
            .collect::<Vec<_>>()
    });
    let state =
        serde_json::json!({"accounts":public,"busy":busy,"blocked":blocked,"message":message});
    let _ = view.evaluate_script(&format!("window.render({state});"));
}
fn work(proxy: EventLoopProxy<AppEvent>, task: impl FnOnce() -> WorkResult + Send + 'static) {
    std::thread::spawn(move || {
        let _ = proxy.send_event(AppEvent::Worked(task()));
    });
}
fn save_accounts(
    proxy: EventLoopProxy<AppEvent>,
    path: PathBuf,
    accounts: Vec<Account>,
    message: String,
) {
    work(proxy, move || {
        store::save(&path, &accounts)?;
        Ok((Some(accounts), message))
    });
}
fn run() -> Result<(), String> {
    let app_mutex =
        platform::MutexGuard::create("Local\\PersonalRobloxAccountController_Rust", true)?;
    let roblox_mutex = platform::MutexGuard::create("ROBLOX_singletonEvent", false)?;
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
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
            }],
        )?;
        path
    } else {
        store::location()?
    };
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Personal Roblox Account Controller")
        .with_visible(!smoke)
        .with_inner_size(LogicalSize::new(1050., 780.))
        .build(&event_loop)
        .map_err(|_| "Could not create the application window.")?;
    let profile = tempfile::Builder::new()
        .prefix("prac-ui-")
        .tempdir()
        .map_err(|_| "Could not create the UI profile.")?;
    let mut context = WebContext::new(Some(profile.path().to_path_buf()));
    let ipc_proxy = proxy.clone();
    let ui_url = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(include_str!("ui.html"))
    );
    let allowed_url = ui_url.clone();
    let view = WebViewBuilder::new_with_web_context(&mut context)
        .with_incognito(true)
        .with_devtools(false)
        .with_url(&ui_url)
        .with_navigation_handler(move |url| url == allowed_url)
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if request.body().len() <= 65536 {
                if let Ok(command) = serde_json::from_str(request.body()) {
                    let _ = ipc_proxy.send_event(AppEvent::Command(command));
                }
            }
        })
        .build(&window)
        .map_err(|_| "Could not initialize WebView2. Check the runtime installation.")?;
    let mut ui = Some(Browser {
        view: Some(view),
        context: Some(context),
        profile: Some(profile),
        window,
        login: false,
        started: Instant::now(),
        next_poll: Instant::now(),
    });
    let mut browsers: Vec<Browser> = Vec::new();
    let mut accounts: Vec<Account> = Vec::new();
    let mut busy = true;
    let mut blocked = false;
    let mut initialized = false;
    let mut message = "Loading accounts...".to_owned();
    let smoke_proxy = proxy.clone();
    if smoke {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            let _ = smoke_proxy.send_event(AppEvent::Command(Command {
                op: "smoke_timeout".into(),
                index: None,
                value: String::new(),
            }));
        });
    }
    event_loop.run(move |event, target, control| {
        let _keep_test_store_alive = &smoke_profile;
        let _keep_mutexes_alive = (&app_mutex, &roblox_mutex);
        *control = if !busy { browsers.iter().filter(|b| b.login).map(|b| b.next_poll).min().map(ControlFlow::WaitUntil).unwrap_or(ControlFlow::Wait) } else { ControlFlow::Wait };
        let Some(main) = ui.as_ref() else { return; };
        let view = main.view.as_ref().expect("live UI");
        match event {
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } => {
                if window_id == main.window.id() {
                    if busy { message = "Please wait for the current operation to finish before closing.".into(); }
                    else { *control = ControlFlow::Exit; }
                } else { browsers.retain(|b| b.window.id() != window_id); }
                render(view,None,busy,blocked,&message);
            }
            Event::UserEvent(AppEvent::Command(command)) => {
                if smoke && command.op.starts_with("smoke_") {
                    if command.op == "smoke_ok" { println!("Synthetic WebView2 UI smoke test passed."); *control = ControlFlow::Exit; }
                    else { eprintln!("Synthetic WebView2 UI smoke test failed: {}.", command.op); std::process::exit(1); }
                    return;
                }
                if command.op == "ready" { if smoke { println!("Smoke: UI ready."); }
                    if !initialized {
                        initialized = true; let proxy = proxy.clone(); let path = path.clone();
                        std::thread::spawn(move || { let _ = proxy.send_event(AppEvent::Loaded(store::load(&path))); });
                    }
                    render(view,Some(&accounts),busy,blocked,&message); return;
                }
                if busy || blocked { return; }
                let selected = command.index.and_then(|i| accounts.get(i)).cloned();
                let result: Result<(),String> = (|| {
                    match command.op.as_str() {
                        "login" => {
                            if browsers.iter().any(|b| b.login) { return Err("A login window is already open. Finish or close it first.".into()); }
                            browsers.push(browser(target,None)?); message = "Sign in on Roblox's page. Close the login window to cancel.".into();
                        }
                        "browser" => { let account = selected.ok_or("Select an account first.")?; browsers.push(browser(target,Some(&account.cookie))?); message = "Account browser opened.".into(); }
                        "import" => {
                            let path = path.clone(); let mut next = accounts.clone();
                            busy = true; message = "Importing accounts...".into();
                            work(proxy.clone(), move || {
                                let file = std::fs::File::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../accountdata.csv")).map_err(|_| "Could not open accountdata.csv in the original project folder.")?;
                                let count = store::import_reader(file,&mut next)?; store::save(&path,&next)?;
                                Ok((Some(next),format!("Imported {count} accounts. Existing usernames were skipped; the original CSV is unchanged.")))
                            });
                        }
                        "description" => {
                            selected.ok_or("Select an account first.")?;
                            if command.value.len() > 40000 { return Err("Description is too long.".into()); }
                            let mut next=accounts.clone(); next[command.index.unwrap()].description=command.value;
                            busy=true; message="Saving description...".into(); save_accounts(proxy.clone(),path.clone(),next,"Description saved.".into());
                        }
                        "place" | "user" => {
                            let account=selected.ok_or("Select an account first.")?;
                            let target=if command.op=="place" { roblox::Target::Place(command.value.trim().parse::<u64>().ok().filter(|n| *n>0).ok_or("Enter a positive numeric place ID.")?) }
                                else { let name=command.value.trim(); if name.is_empty() || name.len()>100 { return Err("Enter a player username.".into()); } roblox::Target::User(name.into()) };
                            busy=true; message="Requesting Roblox launch...".into();
                            work(proxy.clone(),move || { roblox::join(account,target)?; Ok((None,"Launch sent to Roblox.".into())) });
                        }
                        _ => return Err("Unknown action.".into()),
                    }
                    Ok(())
                })();
                if let Err(error)=result { message=error; }
                render(view,None,busy,blocked,&message);
            }
            Event::UserEvent(AppEvent::Loaded(result)) => { if smoke { println!("Smoke: store loaded."); }
                busy=false;
                match result { Ok(loaded)=>{accounts=loaded; message="Ready. Select an account or add one.".into();}, Err(error)=>{blocked=true; message=error;} }
                render(view,Some(&accounts),busy,blocked,&message);
                if smoke {
                    let _ = view.evaluate_script(r#"(() => {
                        try {
                            if(document.querySelectorAll('.account').length!==1) throw 0;
                            if(!document.querySelector('[data-op=browser]').disabled) throw 0;
                            document.querySelector('.account').click();
                            if(document.querySelector('[data-op=browser]').disabled) throw 0;
                            if(document.querySelector('#description').value!=='<script>unsafe</script>\nTest note') throw 0;
                            if(document.querySelector('.account script')) throw 0;
                            if(document.body.textContent.includes('synthetic-cookie-never-sent')) throw 0;
                            document.querySelector('#theme').click();
                            if(!document.body.classList.contains('light')) throw 0;
                            send('smoke_ok');
                        } catch (_) { send('smoke_failed'); }
                    })();"#);
                }
            }
            Event::UserEvent(AppEvent::Worked(result)) => {
                busy=false;
                match result { Ok((updated,status))=>{message=status; if let Some(updated)=updated { accounts=updated; render(view,Some(&accounts),busy,blocked,&message); }}, Err(error)=>message=error }
                render(view,None,busy,blocked,&message);
            }
            Event::UserEvent(AppEvent::Validated(id,result)) => {
                busy=false;
                if let Some(index)=browsers.iter().position(|b| b.window.id()==id) {
                    match result {
                        Ok(mut account)=>{
                            let mut next=accounts.clone();
                            if let Some(old)=next.iter_mut().find(|a| a.username.eq_ignore_ascii_case(&account.username)) { account.description=old.description.clone(); *old=account; } else {next.push(account);}
                            browsers.remove(index); busy=true; message="Login verified. Saving encrypted session...".into();
                            save_accounts(proxy.clone(),path.clone(),next,"Account saved. Ready to play.".into());
                        }
                        Err(error)=>{browsers[index].next_poll=Instant::now()+Duration::from_secs(15); message=format!("{error} Login will retry in 15 seconds.");}
                    }
                } else { message="Login canceled. No account was saved.".into(); }
                render(view,None,busy,blocked,&message);
            }
            Event::MainEventsCleared if !busy && !blocked => {
                for browser in browsers.iter_mut().filter(|b| b.login) {
                    if browser.started.elapsed()>=Duration::from_secs(600) { browser.login=false; browser.window.set_title("Login timed out - close and sign in again"); message="Login timed out. Close the browser and try again.".into(); render(view,None,busy,blocked,&message); continue; }
                    if Instant::now()<browser.next_poll {continue;}
                    browser.next_poll=Instant::now()+Duration::from_secs(2);
                    if let Ok(cookies)=browser.view.as_ref().unwrap().cookies_for_url("https://www.roblox.com/") {
                        if let Some(cookie)=cookies.iter().find(|c| c.name()==".ROBLOSECURITY" && !c.value().is_empty()) {
                            let cookie=cookie.value().to_owned(); let id=browser.window.id(); let proxy=proxy.clone(); busy=true;
                            message="Validating login...".into(); render(view,None,busy,blocked,&message);
                            std::thread::spawn(move || {let _=proxy.send_event(AppEvent::Validated(id,roblox::validate(cookie)));}); break;
                        }
                    }
                }
            }
            Event::LoopDestroyed => { browsers.clear(); drop(ui.take()); }
            _=>{}
        }
        // State may have changed in this event; schedule using the new state.
        if !matches!(*control,ControlFlow::Exit) {
            *control=if !busy && !blocked {browsers.iter().filter(|b|b.login).map(|b|b.next_poll).min().map(ControlFlow::WaitUntil).unwrap_or(ControlFlow::Wait)} else {ControlFlow::Wait};
        }
    });
}
fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
