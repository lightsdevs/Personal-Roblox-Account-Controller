use std::time::{Duration, Instant};

use reqwest::{blocking::Client, header, redirect::Policy};
use serde::Deserialize;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Deserialize)]
struct Account {
    #[serde(rename = "Name")]
    username: String,
    #[serde(rename = "UserId")]
    user_id: u64,
}

enum AppEvent {
    Validated(Result<Account, String>),
}

// Match the Python app's endpoint. Never log headers, cookies, response bodies,
// or raw request errors, which might include sensitive information.
fn validate_session(cookie: String) -> Result<Account, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "Could not initialize the HTTP client.".to_owned())?;
    let mut cookie_header = header::HeaderValue::from_str(&format!(".ROBLOSECURITY={cookie}"))
        .map_err(|_| "The session cookie has an invalid format.".to_owned())?;
    cookie_header.set_sensitive(true);
    let response = client
        .get("https://www.roblox.com/my/account/json")
        .header(header::COOKIE, cookie_header)
        .send()
        .map_err(|_| "Account validation failed (network or timeout).".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "Account validation returned HTTP {}.",
            response.status()
        ));
    }
    let account: Account = response
        .json()
        .map_err(|_| "Roblox did not return an authenticated account record.".to_owned())?;
    if account.user_id == 0 || account.username.trim().is_empty() {
        return Err("Roblox returned an empty account identity.".to_owned());
    }
    Ok(account)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "windows") {
        return Err("This prototype requires Windows and the WebView2 runtime.".into());
    }

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Roblox login prototype - sign in normally")
        .with_inner_size(LogicalSize::new(1100.0, 800.0))
        .build(&event_loop)?;

    // Isolate this run from Edge and every other prototype instance.
    let profile = tempfile::Builder::new()
        .prefix("roblox-login-prototype-")
        .tempdir()?;
    let mut context = WebContext::new(Some(profile.path().to_path_buf()));
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_incognito(true)
        .with_devtools(false)
        .with_url("https://www.roblox.com/login")
        .build(&window)?;
    // Retain resources through the event loop, then drop in this order.
    let mut webview = Some(webview);
    let mut context = Some(context);
    let mut profile = Some(profile);
    let started = Instant::now();
    let mut next_poll = Instant::now() + POLL_INTERVAL;
    let mut validating = false;
    let mut finished = false;
    let mut cookie_error_reported = false;

    println!("Sign in on the Roblox page. CAPTCHA and verification happen there.");
    println!("This test validates the session but does not save accounts or print cookies.");
    println!("Close the window to cancel. Login detection stops after ten minutes.");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = if finished || validating {
            ControlFlow::Wait
        } else {
            ControlFlow::WaitUntil(next_poll)
        };

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(AppEvent::Validated(result)) => {
                validating = false;
                match result {
                    Ok(account) => {
                        let message = format!(
                            "Login verified: {} (user ID {}). Close this window to finish.",
                            account.username, account.user_id
                        );
                        println!("{message}");
                        window.set_title(&message);
                        finished = true;
                        *control_flow = ControlFlow::Wait;
                    }
                    Err(message) => {
                        eprintln!("{message} Retrying in 15 seconds.");
                        window.set_title("Roblox login prototype - validation failed; retrying");
                        next_poll = Instant::now() + Duration::from_secs(15);
                        *control_flow = ControlFlow::WaitUntil(next_poll);
                    }
                }
            }
            Event::MainEventsCleared if !finished && !validating => {
                if started.elapsed() >= LOGIN_TIMEOUT {
                    finished = true;
                    window.set_title("Login detection timed out - close and rerun to retry");
                    eprintln!("Login detection timed out. Close and rerun to retry.");
                    *control_flow = ControlFlow::Wait;
                    return;
                }
                if Instant::now() < next_poll {
                    return;
                }
                next_poll = Instant::now() + POLL_INTERVAL;
                *control_flow = ControlFlow::WaitUntil(next_poll);

                // Poll outside WebView callbacks. A redirect is not proof of
                // login; JavaScript cannot read HttpOnly session cookies.
                let Some(browser) = webview.as_ref() else {
                    return;
                };
                match browser.cookies_for_url("https://www.roblox.com/") {
                    Ok(cookies) => {
                        cookie_error_reported = false;
                        if let Some(cookie) = cookies.iter().find(|cookie| {
                            cookie.name() == ".ROBLOSECURITY" && !cookie.value().is_empty()
                        }) {
                            let session = cookie.value().to_owned();
                            let proxy = proxy.clone();
                            validating = true;
                            *control_flow = ControlFlow::Wait;
                            window.set_title("Roblox login prototype - validating session...");
                            std::thread::spawn(move || {
                                let result = validate_session(session);
                                let _ = proxy.send_event(AppEvent::Validated(result));
                            });
                        }
                    }
                    Err(_) if !cookie_error_reported => {
                        cookie_error_reported = true;
                        eprintln!("WebView2 could not read cookies yet; retrying.");
                    }
                    Err(_) => {}
                }
            }
            Event::LoopDestroyed => {
                drop(webview.take());
                drop(context.take());
                if let Some(directory) = profile.take() {
                    let path = directory.path().to_path_buf();
                    if directory.close().is_err() {
                        // WebView2 can retain file locks briefly after closing.
                        eprintln!("Temporary browser files remain at {}", path.display());
                    }
                }
            }
            _ => {}
        }
    });
}
