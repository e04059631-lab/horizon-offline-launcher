$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut("C:\Users\7ims (Admin)\horizon-offline-launcher\screenshot_launcher.lnk")
$Shortcut.TargetPath = "C:\Users\7ims (Admin)\horizon-offline-launcher\horizon-offline-launcher.exe"
$Shortcut.Arguments = "--screenshot-all"
$Shortcut.WorkingDirectory = "C:\Users\7ims (Admin)\horizon-offline-launcher"
$Shortcut.Save()
Write-Host "Shortcut created successfully!"
