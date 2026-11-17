# WebView2 login prototype

Standalone Rust experiment for replacing Selenium login. Opens the real Roblox
login page in WebView2, polls the native cookie API for `.ROBLOSECURITY`, then
validates it against the same account endpoint used by the Python app. Successful
validation shows the username and user ID in the window title and terminal.

The Python app, game launcher, mutex and `accountdata.csv` are unchanged. This
prototype does not export cookies, save accounts, or implement game launching.
Each run uses a separate temporary browser directory and InPrivate mode. Closing
the window attempts to delete that directory; a cleanup failure prints its path.
Cookies are never deliberately printed or written to an account file.

## Build and run (Windows)

Prerequisites:

- [Rust stable](https://www.rust-lang.org/tools/install), using the default MSVC toolchain.
- Visual Studio Build Tools with **Desktop development with C++**, including the
  MSVC compiler and Windows SDK (the Rust installer may offer to install these).
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

From the repository root, in a terminal that has Cargo on PATH:

```powershell
.\login-prototype\run.ps1
```

Or run directly, without the PowerShell wrapper:

```powershell
cargo run --manifest-path .\login-prototype\Cargo.toml
```

The first build downloads dependencies. Normal sign-in and verification are
completed by you in the browser window. Validation runs on a background thread;
failed validation retries every 15 seconds. Detection stops after ten minutes.
Close and rerun for another attempt or account.

## Acceptance checks

1. Open the prototype and confirm the Roblox login page loads.
2. Complete a normal login; confirm the title and terminal show the correct
   username and user ID. A URL change alone must not count as success.
3. Exercise any CAPTCHA and two-step verification your account requires. These
   flows, including popup/passkey behavior, require live testing; their WebView2
   compatibility is not established by this prototype's source code.
4. Close and rerun: confirm you get a fresh login session. Try a second account.
5. Close before logging in: confirm cancellation works. Disconnect the network
   during validation to check that the window remains responsive.
6. Confirm the existing Python app still sees the same saved accounts.

If the browser authenticates but validation fails, distinguish HTTP/account
endpoint problems from WebView cookie extraction. The terminal reports status
codes without response bodies or credentials. Do not share session cookies.

## Validation status

The user reported a successful live login on November 17, 2026. Final account
validation and CAPTCHA/two-step/passkey coverage were not explicitly confirmed.
Rust and build tools are now available. Continued application work lives in
[`../rust-app`](../rust-app/README.md), which has separate validation notes.

Once build prerequisites are installed, run:

```powershell
cargo fmt --manifest-path .\login-prototype\Cargo.toml --check
cargo check --manifest-path .\login-prototype\Cargo.toml
cargo clippy --manifest-path .\login-prototype\Cargo.toml -- -D warnings
```

Commit the generated `Cargo.lock` after the first successful dependency resolution.
