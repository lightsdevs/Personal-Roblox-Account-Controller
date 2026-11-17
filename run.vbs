Option Explicit
Dim shell, files, folder, powershell, result, command, logPath
Set shell = CreateObject("WScript.Shell")
Set files = CreateObject("Scripting.FileSystemObject")
folder = files.GetParentFolderName(WScript.ScriptFullName)
shell.CurrentDirectory = folder
powershell = shell.ExpandEnvironmentStrings("%SystemRoot%") & "\System32\WindowsPowerShell\v1.0\powershell.exe"
logPath = folder & "\startup.log"
command = Quote(powershell) & " -NoLogo -NoProfile -ExecutionPolicy Bypass -File " & Quote(folder & "\run.ps1") & " > " & Quote(logPath) & " 2>&1"
result = shell.Run(Quote(shell.ExpandEnvironmentStrings("%ComSpec%")) & " /d /s /c " & Quote(command), 0, True)
If result <> 0 Then
    MsgBox "The Rust app could not run. Error details are in:" & vbCrLf & logPath & vbCrLf & "Check the build prerequisites in README.md.", vbCritical, "Unable to start app"
End If
WScript.Quit result
Function Quote(value)
    Quote = Chr(34) & value & Chr(34)
End Function
