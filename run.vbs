Option Explicit

Dim shell, files, folder, python, candidate, result, command, logPath
Set shell = CreateObject("WScript.Shell")
Set files = CreateObject("Scripting.FileSystemObject")
folder = files.GetParentFolderName(WScript.ScriptFullName)
shell.CurrentDirectory = folder
python = ""

For Each candidate In Array(Quote(folder & "\.venv\Scripts\python.exe"), "py -3", "python")
    On Error Resume Next
    Err.Clear
    result = shell.Run(candidate & " --version", 0, True)
    If Err.Number = 0 Then
        If result = 0 Then python = candidate
    End If
    On Error GoTo 0
    If python <> "" Then Exit For
Next

If python = "" Then
    MsgBox "Python 3 could not be found. Install Python, then follow the setup steps in README.md.", vbCritical, "Unable to start app"
    WScript.Quit 1
End If

logPath = folder & "\startup.log"
command = python & " " & Quote(folder & "\main.py") & " > " & Quote(logPath) & " 2>&1"
result = shell.Run(Quote(shell.ExpandEnvironmentStrings("%ComSpec%")) & " /d /s /c " & Quote(command), 0, True)
If result <> 0 Then
    MsgBox "The app could not run. Error details are in:" & vbCrLf & logPath & vbCrLf & vbCrLf & "For missing modules, follow the setup steps in README.md.", vbCritical, "Unable to start app"
End If
WScript.Quit result

Function Quote(value)
    Quote = Chr(34) & value & Chr(34)
End Function
