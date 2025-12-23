# AUR Publishing

## Initial Setup (one-time)

1. Create account at https://aur.archlinux.org
2. Add SSH public key to account settings
3. Clone AUR repo (note: AUR uses `master`, not `main`):
   ```bash
   git clone ssh://aur@aur.archlinux.org/hyprpier-git.git /tmp/hyprpier-aur
   cd /tmp/hyprpier-aur
   git branch -m main master  # if needed
   ```
4. Copy files and push:
   ```bash
   cp /path/to/hyprpier/PKGBUILD .
   cp /path/to/hyprpier/.SRCINFO .
   git add PKGBUILD .SRCINFO
   git commit -m "Initial upload"
   git push -u origin master
   ```

## Pushing Updates

AUR `-git` packages pull from your GitHub repo at install time, so users get the latest code automatically when they run `yay -S hyprpier-git`.

However, you must update the AUR repo if you change:
- `PKGBUILD` (dependencies, build steps, metadata)
- Package description or URL

You should also update after pushing new commits to avoid false "update available" prompts (the `pkgver()` function generates version from git history).

To update:
```bash
cd /path/to/hyprpier

# Regenerate pkgver from git history
makepkg -o
makepkg --printsrcinfo > .SRCINFO

# Commit and push to GitHub
git add PKGBUILD .SRCINFO
git commit -m "Update pkgver"
git push origin main

# Push to AUR
cd /tmp/hyprpier-aur  # or wherever you cloned it
cp /path/to/hyprpier/PKGBUILD .
cp /path/to/hyprpier/.SRCINFO .
git add PKGBUILD .SRCINFO
git commit -m "Update pkgver"
git push
```

## Testing Locally

Build and install without pushing to AUR:
```bash
cd /path/to/hyprpier
makepkg -si
```

## Useful Links

- AUR package: https://aur.archlinux.org/packages/hyprpier-git
- AUR guidelines: https://wiki.archlinux.org/title/AUR_submission_guidelines
