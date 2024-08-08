Set WshShell = CreateObject("WScript.Shell")
CurrentDirectory = WshShell.CurrentDirectory
WshShell.Run "cmd.exe /c cd " & Chr(34) & CurrentDirectory & "\" & Chr(34) & " && python main.py", 0
Set WshShell = Nothing
