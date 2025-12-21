# TODO

## Potential Enhancements

### Multiple Dock Support
Currently uses the first matching dock when multiple are connected. Could support:
- Priority ordering for docks
- Profile per dock combination

### Profile Export/Import
Add commands for backup and sharing:
- `hyprpier export <profile>` - Export to standalone JSON
- `hyprpier import <file>` - Import from file

### Monitor Preview
Visual ASCII representation of monitor layout in the TUI arrange screen.

### Scale/Transform Editing
Add TUI controls for:
- Monitor scale (1.0, 1.25, 1.5, 2.0, etc.)
- Transform (rotate, flip)

Currently requires manual JSON editing.

### Refresh Rate Selection
Allow selecting refresh rate in TUI when multiple rates are available for a resolution.

### Non-Thunderbolt Dock Support
Detect USB-C docks that don't use Thunderbolt. Could use:
- USB device IDs
- Display port MST hub detection
- Manual dock registration by user

## AUR Publishing

See [docs/aur.md](docs/aur.md) for setup and update instructions.
