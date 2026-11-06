# Contributing to Vox Launcher

Thanks for wanting to help with Vox Launcher! Bug reports, ideas, docs fixes and code are all welcome.

Not sure where to start? Join the [Discord](https://discord.gg/vox-launcher) and say hi in the support channels.

## Contents

- [Ways to help](#ways-to-help)
- [Before you start](#before-you-start)
- [Reporting a bug](#reporting-a-bug)
- [Suggesting a feature](#suggesting-a-feature)
- [Setting up the project](#setting-up-the-project)
- [Making a change](#making-a-change)
- [Pull request checklist](#pull-request-checklist)
- [Code style](#code-style)
- [Commit messages](#commit-messages)
- [Security issues](#security-issues)
- [License and your contributions](#license-and-your-contributions)

## Ways to help

- 🐛 **Report bugs**, especially with clear steps to reproduce
- 💡 **Suggest features** and improvements
- 💥 **Share crash reports** so the crash helper can learn new fixes
- 🧪 **Test new builds** and tell us what broke
- 📝 **Improve the docs**, README and website text
- 🎨 **Design** themes, layouts and icons
- 💻 **Write code** for bug fixes and features

## Before you start

- **Be kind.** Treat everyone with respect. Harassment, hate and personal attacks aren't tolerated here or on the Discord.
- **Search first.** Check the [open issues](https://github.com/sylphiras/vox-launcher/issues) and [pull requests](https://github.com/sylphiras/vox-launcher/pulls) in case someone already raised it.
- **Talk before big changes.** For anything large, such as a new feature, a big refactor or a UI redesign, open an issue first so we can agree on the approach before you spend time on it. Small fixes can go straight to a pull request.

## Reporting a bug

[Open a bug report](https://github.com/sylphiras/vox-launcher/issues/new) and include:

1. **What happened**, and what you expected instead.
2. **Steps to reproduce**, as exact as possible.
3. **Your setup:** Vox Launcher version, Windows version, Minecraft version, mod loader (Fabric, Quilt, Forge or NeoForge) and whether mods or modpacks are involved.
4. **Logs or a crash report.** Use the crash helper's copy button, or attach the log files.
5. **Screenshots** if it's a visual problem.

Please don't post account tokens, passwords or personal information. Remove them from logs before sharing.

Support questions such as "how do I install a mod?" are better asked in the Discord than on GitHub.

## Suggesting a feature

Open an issue and describe:

- **The problem** you're trying to solve, not just the solution.
- **Your idea** for how it could work.
- **Alternatives** you've considered.

Ideas are also welcome in the Discord's feature-requests channel. Not every idea fits the project, but all of them are read.

## Setting up the project

> **Maintainer note:** fill in the exact commands below for your stack before publishing this file.

**Requirements**

- Windows 10 or 11
- Git
- `<runtime / toolchain and version, for example Node.js 22, Rust stable, .NET 9>`
- A Minecraft: Java Edition account is only needed to test online play. Offline profiles work for most testing.

**Get the code**

```bash
git clone https://github.com/<your-username>/vox-launcher.git
cd vox-launcher
```

**Install and run**

```bash
# install dependencies
<install command>

# run in development mode
<dev command>

# build a release
<build command>
```

**Landing page** (`index.php`, `config.php` and `assets/`): run it with PHP 8.4:

```bash
php -S localhost:8000
```

Site content is in `config.php`, and `index.php` only renders it.

## Making a change

1. **Fork** the repository and clone your fork.
2. **Create a branch** from `main` with a short, descriptive name:
   - `fix/crash-helper-java-detection`
   - `feat/instance-sharing`
   - `docs/contributing-typo`
3. **Make your change.** Keep it focused: one fix or feature per pull request.
4. **Test it.** Run the launcher, try the change by hand, and run any automated tests.
5. **Commit** following the [commit message guide](#commit-messages).
6. **Push** to your fork and **open a pull request** against `main`.

## Pull request checklist

Before you open a pull request, check that:

- [ ] It solves one problem and doesn't mix in unrelated changes
- [ ] It builds and runs without errors
- [ ] You tested it on Windows
- [ ] New or changed UI works with all four themes and layouts
- [ ] No secrets, tokens, personal paths or debug leftovers are included
- [ ] Docs or the README are updated if behaviour changed
- [ ] The description says **what** changed and **why**, and links the issue (for example `Fixes #123`)
- [ ] Screenshots or a short clip are attached for visual changes

A maintainer will review it, may ask for changes and will merge it when it's ready. Please be patient: this is a small project, and reviews can take a few days.

## Code style

- Match the style of the code around your change.
- Keep functions small and names clear. Comment the *why*, not the *what*.
- Don't add new dependencies without discussing them first. Vox is meant to stay small and fast.
- Don't make launching a game slower. Setting up an instance can do the work once, but pressing **Play** should stay quick.
- For the website: PHP is strict-typed (`declare(strict_types=1)`), all output is escaped with `e()`, and content belongs in `config.php`.

## Commit messages

Short, clear and in the present tense:

```
Fix crash helper missing Java 21 detection
Add .mrpack import progress bar
Update README install steps
```

Prefixes such as `fix:`, `feat:` and `docs:` are welcome but not required. Keep the first line under about 70 characters. Add more detail in the body if it helps.

## Security issues

**Please don't report security problems in a public issue.** If you find a vulnerability, such as something affecting account tokens, downloads or updates, use GitHub's private reporting under the repository's **Security** tab ("Report a vulnerability"), or message a maintainer privately on Discord. We'll look at it as soon as we can and will credit you if you'd like.

## License and your contributions

Vox Launcher's source is available to read, build and change for personal use. Redistributing it, selling it or passing it off as your own isn't allowed. See the [LICENSE](LICENSE) file for the full terms.

By submitting a contribution, you confirm that:

- you wrote it yourself, or have the right to submit it, and
- you agree that it can be used, modified and distributed as part of Vox Launcher under the project's license.

Vox Launcher is not an official Minecraft product and isn't approved by or associated with Mojang Studios or Microsoft.

---

Thanks again for helping make Vox better. 🧱
