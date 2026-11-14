$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut("C:\Users\7ims (Admin)\horizon-offline-launcher\run_launcher.lnk")
$Shortcut.TargetPath = "C:\Users\7ims (Admin)\horizon-offline-launcher\horizon-offline-launcher.exe"
$Shortcut.WorkingDirectory = "C:\Users\7ims (Admin)\horizon-offline-launcher"
$Shortcut.Save()
Write-Host "run_launcher.lnk created!"
