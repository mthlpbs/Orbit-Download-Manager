# Run Orbit without a window on Windows

[← Back to docs index](README.md)

Orbit supports a headless mode on Windows 10 and 11. The same download list, queues,
scheduler, speed limits and torrents can be controlled with the [remote control](remote-control.md),
the browser extension or the command line.

## Start headless mode

Run from Orbit’s installation folder in PowerShell:

```powershell
.\orbitdownloadmanager.exe --headless --data C:\OrbitData --downloads C:\Downloads
```

| Flag | Environment variable | Purpose |
| --- | --- | --- |
| `--data <folder>` | `MDM_DATA_DIR` | Settings, list, certificate and paired devices |
| `--downloads <folder>` | `MDM_DOWNLOADS` | Download folders |
| `--port <number>` | `MDM_REMOTE_PORT` | Remote-control port (default 8463) |
| `--listen <address>` | `MDM_REMOTE_LISTEN` | Listening address |
| `--hosts <names>` | `MDM_REMOTE_HOSTS` | Additional permitted hostnames |

Remote control is enabled in this mode. Set `MDM_REMOTE=off` to disable it. Use the same
data folder when controlling this instance from the CLI:

```powershell
$env:MDM_DATA_DIR = 'C:\OrbitData'
.\orbitdownloadmanager-cli.exe pair
```

## Limits

Headless mode cannot open dialogs. Duplicate downloads and existing files follow the saved
settings. Helpers must be installed separately. Keep remote control on your local network;
use a VPN for access from elsewhere.
