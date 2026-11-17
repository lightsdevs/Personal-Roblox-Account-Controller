# Rust controller

Windows desktop rewrite of the Python app, using Rust and egui/eframe for the
main interface. The account list, forms, and buttons are native-rendered widgets;
there is no HTML or JavaScript frontend. Roblox sign-in and account browsing use
separate WebView2 windows. The Python app and login prototype remain available
independently.

## Run

Requires the Rust MSVC toolchain, Visual Studio C++ build tools, and Microsoft
Edge WebView2 Runtime for Roblox browser windows (the same prerequisites as the
login prototype). The main interface uses eframe's OpenGL renderer.

From the repository root:

```powershell
.\rust-app\run.ps1
```

The launcher builds and runs the release version. After building, you can also
run `rust-app\target\release\personal-roblox-account-controller.exe` directly.
No Python, Selenium, or EdgeDriver is required for the Rust app.

## Using it

- **Add / sign in again** opens a fresh isolated Roblox login window. Sign in on
  Roblox's own page. The app validates the account before saving its session.
  Signing in again updates that username's session and preserves its description.
- **Import old CSV** reads `accountdata.csv` from the original project folder
  (the repository location at build time), only after you choose this action.
  It imports usernames, sessions, and descriptions; duplicate usernames are
  skipped case-insensitively. It does not validate imported sessions immediately.
  The original CSV is never modified or deleted.
- Click an account to select it, Ctrl-click to toggle individual accounts, or
  Shift-click to select a range. Ctrl+Shift-click adds a range to the selection.
- Joining by place ID or player username applies to every selected account,
  sequentially. Progress names the current account; the final result reports
  successful launch requests and individual failures. An expired session needs
  another login, but does not prevent the remaining accounts from being attempted.
- Save description applies the entered text to every selected account in one
  encrypted store update. If their descriptions differ, enter a replacement or
  choose **Use empty description for all selected**, then save.
- **Open account browser** opens an isolated embedded WebView2 window signed in
  as the selected account (requires exactly one selected account). This replaces the Python app's standalone Edge
  window. It closes with the controller. Popups and non-HTTPS navigation are
  blocked; popup-based sign-in flows have not been verified.
- **Light / dark** changes the current window's theme.
- The account list and controls appear side by side in wide windows and stack
  in narrow windows. Scroll vertically to reach controls in short windows.

Network requests and account-store reads/writes run on worker threads. Roblox
browser windows have their own UI thread; WebView2 starts only when needed.
The app serializes actions while a request, sign-in, or save is pending. Closing a login window
cancels it; closing the controller during an active operation waits for it to
finish. Login detection polls every two seconds, retries failed validation after
15 seconds, and stops after ten minutes.

## Storage and privacy

Accounts are stored at
`%LOCALAPPDATA%\PersonalRobloxAccountController\accounts.dpapi`. The complete
store is encrypted using Windows DPAPI for the current Windows user, written
through an encrypted temporary file, and replaced atomically. A store that
cannot be decrypted or parsed blocks account operations instead of overwriting
it. Copying the file to a different Windows user is not a supported migration.

Cookies stay in Rust/native browser APIs and are never painted in egui widgets.
Account names and descriptions are displayed as text. HTTP errors
are sanitized; cookies, authentication tickets, headers, and response bodies
are not logged. Roblox requests disable redirects and use HTTPS. The app has
no OpenAI integration. These are implementation properties, not a traffic audit.

The old CSV remains plaintext until you separately remove it. Browser windows
use temporary InPrivate profiles, with cleanup attempted on close. WebView2 may
retain temporary files if it still holds locks. Encryption does not protect
sessions from other software already running as your Windows user.

## Compatibility

The port retains the Python app's account endpoint, CSRF and authentication-ticket
exchange, place validation, username lookup, and Roblox launch protocol fields.
It invokes the registered Roblox protocol directly through Windows, without a
command shell. It holds `ROBLOX_singletonEvent` for the application lifetime,
matching the existing mutex behavior. A separate mutex prevents concurrent Rust
controllers from overwriting the account store. Start the controller before
Roblox when testing multiple clients.

## Verification

On November 17, 2026:

- Cargo check, tests, and Clippy with warnings denied passed.
- Twelve synthetic-data tests cover DPAPI round trips and rejection of invalid
  ciphertext, encrypted store replacement, CSV quoting/multiline/empty values,
  duplicate handling and malformed-import atomicity, both launch URL forms,
  selection preservation, blocked-store state, narrow/short/wide layouts,
  Ctrl/Shift selection, batch description writes, and partial batch join failures.
  Layout tests check that scrolling reaches Save description and that credentials
  are not painted.
- A hidden egui smoke test checks encrypted synthetic account loading, selection,
  description saving, window resizing, theme switching, and creation/cleanup of
  an offline WebView2 window on the browser thread.
- No real saved credentials were read for these checks. No live login or game
  launch was performed with this rewrite. The earlier prototype had a
  user-reported successful login.

Run checks from the repository root:

```powershell
cargo fmt --manifest-path .\rust-app\Cargo.toml --check
cargo check --locked --manifest-path .\rust-app\Cargo.toml
cargo test --locked --manifest-path .\rust-app\Cargo.toml
cargo clippy --locked --manifest-path .\rust-app\Cargo.toml -- -D warnings
cargo run --locked --manifest-path .\rust-app\Cargo.toml -- --smoke-test
```

The smoke mode uses a hidden window and temporary synthetic store; it does not
load the real store, import the CSV, or contact Roblox.

Before relying on the rewrite, check normal login and any CAPTCHA/two-step
verification you use; confirm account save and persistence after restarting;
check that the account browser shows the intended user; and exercise both join
methods and multiple Roblox clients. Also check cancellation and expired-session
errors. Do not paste cookies, tickets, or browser storage into diagnostics.
