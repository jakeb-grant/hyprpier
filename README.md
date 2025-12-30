# Hyprpier

Hyprland monitor profile manager with Thunderbolt dock detection.

## Features

- **Monitor Profiles** - Save and restore complete monitor configurations (resolution, refresh rate, position, scale, transform)
- **Dock Detection** - Automatically detect Thunderbolt docks by UUID
- **Auto-Switching** - Switch profiles automatically when docking/undocking
- **TUI Manager** - Interactive terminal UI for managing profiles
- **Workspace Assignment** - Configure which workspaces belong to which monitors
- **Lid Switch** - Configure laptop lid behavior per profile

## Installation

### AUR (recommended)

```bash
yay -S hyprpier-git
```

### Prebuilt binary

Download from [GitHub Releases](https://github.com/jakeb-grant/hyprpier/releases):

```bash
tar -xzf hyprpier-v*.tar.gz
sudo mv hyprpier /usr/local/bin/
```

### From source

```bash
git clone https://github.com/jakeb-grant/hyprpier.git
cd hyprpier
cargo build --release
sudo cp target/release/hyprpier /usr/local/bin/
```

## Quick Start

```bash
# Launch the TUI manager
hyprpier mgr

# Or use CLI commands
hyprpier list              # List all profiles
hyprpier current           # Show active profile
hyprpier apply <profile>   # Apply a profile
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `hyprpier mgr` | Launch TUI manager |
| `hyprpier apply <name>` | Apply a profile by name |
| `hyprpier apply --auto` | Auto-detect dock and apply linked profile |
| `hyprpier apply --no-runtime` | Generate config only, don't apply via hyprctl |
| `hyprpier list` | List all profiles |
| `hyprpier current` | Show currently active profile |
| `hyprpier thunderbolt --list` | List Thunderbolt devices |
| `hyprpier thunderbolt --status` | Show Thunderbolt security mode |
| `hyprpier setup` | Install udev rules for auto-switching |
| `hyprpier setup --uninstall` | Remove udev rules |
| `hyprpier setup --resume` | Install resume fix service (resets Thunderbolt on wake) |
| `hyprpier setup --resume --uninstall` | Remove resume fix service |
| `hyprpier daemon` | Start the background daemon |

## TUI Keybindings

### Profile List
| Key | Action |
|-----|--------|
| `n` | New profile |
| `e` / `Enter` | Edit profile |
| `d` | Delete profile |
| `a` | Apply profile |
| `u` | Set as undocked fallback |
| `t` | Thunderbolt manager |
| `j/k` | Navigate |
| `q` | Quit |

### Profile Editor
| Key | Action |
|-----|--------|
| `d` | Detect current monitors |
| `a` | Arrange monitors |
| `l` | Link/unlink dock |
| `s` | Save profile |
| `Tab` | Next field |
| `Esc` | Back |

### Monitor Arrangement
| Key | Action |
|-----|--------|
| `h/l` | Move monitor left/right |
| `d` | Toggle monitor enabled |
| `x` | Remove monitor |
| `0-9` | Assign workspace |
| `s` | Save changes |
| `Esc` | Cancel |

### Thunderbolt Manager
| Key | Action |
|-----|--------|
| `x` | Unlink dock |
| `s` | Toggle auto-switch setup |
| `r` | Toggle resume fix service |
| `Tab` | Switch sections |

## Auto-Switching Setup

To enable automatic profile switching when docking/undocking:

```bash
# Install udev rules (requires sudo)
sudo hyprpier setup
```

Alternatively, press `s` in the Thunderbolt Manager (TUI) to toggle auto-switch setup.

**Note:** The udev rule stores the absolute path to the binary. If you move or rebuild hyprpier, re-run `sudo hyprpier setup` to update the path.

Then start the daemon using one of the methods below.

### Hyprland (Recommended)

Add to `~/.config/hypr/hyprland.conf`:

```bash
exec-once = hyprpier daemon
```

The daemon starts with Hyprland and stops when the session ends.

### Systemd (Alternative)

For non-Hyprland setups or if you prefer systemd management:

Create `~/.config/systemd/user/hyprpier.service`:

```ini
[Unit]
Description=Hyprpier Monitor Profile Daemon
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/hyprpier daemon
Restart=on-failure

[Install]
WantedBy=graphical-session.target
```

Then:

```bash
systemctl --user enable --now hyprpier
```

## Configuration

Profiles are stored in `~/.config/hyprpier/`:

```
~/.config/hyprpier/
├── .metadata.json      # Active profile, dock links, undocked profile
├── laptop.json         # Profile files
├── docked.json
└── ...
```

Hyprland config is written to `~/.config/hypr/monitors.conf`. Source it from your main config:

```bash
# hyprland.conf
source = ~/.config/hypr/monitors.conf
```

## How It Works

1. **Profile Creation** - Detect current monitors via `hyprctl`, save their configuration
2. **Dock Linking** - Associate a profile with a Thunderbolt dock's UUID
3. **Auto-Detection** - When dock connects, daemon detects it via udev and applies the linked profile
4. **Fallback** - When no dock is detected, applies the "undocked" profile if set

Monitor descriptions (hardware names) are stored to handle port name changes across reconnections.

**Note:** Currently only one dock at a time is supported. If multiple docks are connected, the first one with a linked profile is used.

## Security Considerations

**Thunderbolt Auto-Authorization:** When you run `hyprpier setup`, it installs udev rules that automatically authorize ALL Thunderbolt devices on connection. This bypasses Thunderbolt security to enable seamless dock detection.

This is convenient for personal use but has security implications:
- Any Thunderbolt device plugged in is automatically trusted
- Potential for DMA attacks in high-security environments

If you require Thunderbolt security, do not use `hyprpier setup`. You can still use Hyprpier for manual profile management.

## Troubleshooting

**Dock not detected after resume from sleep:**

Some Thunderbolt controllers fail to wake from D3hot sleep state. Install the resume fix service for automatic recovery:

```bash
sudo hyprpier setup --resume
```

Or press `r` in the Thunderbolt Manager (TUI). This installs a systemd service that resets the Thunderbolt controller on wake, before it can get stuck.

The udev rules also include reactive fixes (PCI rescan, force power via WMI) but these only work if the controller can generate events. The resume service is more reliable.

If you still have issues, try manually rescanning:
```bash
sudo sh -c 'echo 1 > /sys/bus/pci/rescan'
```

Then replug the dock. If this consistently fails, the controller may need a reboot to recover.

**Daemon not receiving events:**
```bash
# Check if daemon is running
pgrep -f "hyprpier daemon"

# Check udev rules are installed
cat /etc/udev/rules.d/99-hyprpier.rules

# Test manually
hyprpier notify
```

**Profile not applying:**
```bash
# Check Hyprland is running
hyprctl monitors

# Apply with verbose output
hyprpier apply --auto
```

**Multiple notifications on dock:**
- This is normal - udev fires multiple events
- The daemon debounces and only applies once
- Duplicate notifications are suppressed if profile is already active

## License

MIT
