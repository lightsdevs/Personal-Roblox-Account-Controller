# Rust controller

Windows desktop rewrite of the Python app, using Rust and WebView2. The Python
app and login prototype remain available independently.

## Run

Requires the Rust MSVC toolchain, Visual Studio C++ build tools, and Microsoft
Edge WebView2 Runtime (the same prerequisites as the login prototype).

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
- Select an account to edit its description, join a place ID, or follow a player
  by username. An expired session needs another login.
- **Open account browser** opens an isolated embedded WebView2 window signed in
  as the selected account. This replaces the Python app's standalone Edge
  window. It closes with the controller. Popups and non-HTTPS navigation are
  blocked; popup-based sign-in flows have not been verified.
- **Light / dark** changes the current window's theme.

Network requests and account-store reads/writes run on worker threads. The app
serializes actions while a request or save is pending. Closing a login window
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

Cookies stay in Rust/native browser APIs and are not sent to the local UI's
JavaScript. Account names and descriptions are displayed as text. HTTP errors
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
- Five synthetic-data tests cover DPAPI round trips and rejection of invalid
  ciphertext, encrypted store replacement, CSV quoting/multiline/empty values,
  duplicate handling and malformed-import atomicity, and both launch URL forms.
- A hidden WebView2 smoke test passed: encrypted synthetic account loading,
  IPC readiness, selection controls, text rendering of HTML-like descriptions,
  omission of cookies from the UI, and theme switching.
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
