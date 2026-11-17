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

`tkinter` and `csv` are included with the standard Windows Python installation;
they do not need to be installed with pip.
