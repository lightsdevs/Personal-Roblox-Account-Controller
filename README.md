only windows guys sorry

inspired by Roblox Account Manager: https://github.com/ic3w0lf22/Roblox-Account-Manager

account data is locally stored as plaintext: passwords are safe, but cookies are exposed.

Requirements: Python 3 with Tkinter, and Microsoft Edge.

Open PowerShell in this folder and run:

```powershell
py -3 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
```

Then double-click `run.vbs`. It uses the local `.venv` when present and
works even when launched from another folder. If startup fails, it displays
an error and saves details in `startup.log` in this folder.

A standalone Rust/WebView2 login experiment is in
[`login-prototype`](login-prototype/README.md). It tests browser login and session
validation without changing the Python application or saved accounts.

`tkinter` and `csv` are included with the standard Windows Python installation;
they do not need to be installed with pip.

A Rust desktop rewrite is now available in [`rust-app`](rust-app/README.md), with
WebView2 login, encrypted account storage, optional CSV import, descriptions,
account browser windows, and place/player joins. From this repository root, run
`.\rust-app\run.ps1`. Live Roblox acceptance checks
for the rewrite are still pending; the original Python launcher remains available.
