# Security Policy

Thanks for helping keep Vox Launcher and its users safe.

## Supported Versions

Only the latest release of Vox Launcher gets security fixes. Vox updates itself, so please update to the newest version before reporting an issue, to check it hasn't already been fixed.

| Version        | Supported          |
| -------------- | ------------------ |
| Latest release | :white_check_mark: |
| Older releases | :x:                |

The current version is listed on the [releases page](https://github.com/sylphiras/vox-launcher/releases).

## Reporting a Vulnerability

**Please don't report security problems in a public issue, pull request, discussion or Discord channel.** Public reports can put users at risk before a fix is ready.

### How to report

1. Go to the [**Security** tab](https://github.com/sylphiras/vox-launcher/security) of this repository.
2. Click **Report a vulnerability** to open a private report that only the maintainers can see.

If you can't use that, send a private message to a maintainer in the [Discord](https://discord.gg/vox-launcher). Please only say that you have a security report and ask for a private way to send it. Don't post details in a public channel.

### What to include

- A clear description of the problem and what an attacker could do with it
- Steps to reproduce it, or a short proof of concept
- The Vox Launcher version and your Windows version
- Any logs, screenshots or files that help (please remove your own tokens and personal information first)

### What to expect

This is a small project run by a few people, so these are goals rather than guarantees:

| Step                    | Target            |
| ----------------------- | ----------------- |
| Acknowledge your report | within 3 days     |
| First assessment        | within 7 days     |
| Updates while we work   | about every 7 days |
| Fix for confirmed issues | as soon as possible, depending on severity |

If the report is **accepted**, we'll work on a fix, release it, and let you know. We'll credit you in the release notes if you'd like. Please tell us how you want to be named, or whether you'd rather stay anonymous.

If the report is **declined**, we'll explain why, for example because it's expected behaviour, can't be reproduced or is out of scope. You're welcome to reply with more detail.

### Please

- Give us reasonable time to fix the issue before sharing it publicly. We ask for up to 90 days, and we'll usually be much quicker.
- Only test against your own accounts and installations.
- Don't access, change or delete other people's data, and don't disrupt other people's use of the project.

We won't take action against anyone who finds and reports a problem in good faith and follows this policy.

## Scope

**In scope**

- The Vox Launcher desktop application and its installer and updater
- Handling of Microsoft and Minecraft account sign-in and stored tokens
- Downloading, installing and updating mods, modpacks and other files, including `.mrpack` and `.zip` import and export (for example path traversal or unsafe file writes)
- Launching the game and the arguments passed to it
- The Vox Client mod
- The project website (`vox-launcher.com`) and its source in this repository

**Out of scope**

- Vulnerabilities in Minecraft, Mojang or Microsoft services, Modrinth, Discord or other third-party software. Please report these to the relevant vendor.
- Malicious mods or modpacks created by third parties (though weaknesses in how Vox installs or checks them are in scope)
- Problems that need physical access to an unlocked computer, or a machine that is already compromised
- Social engineering, spam or denial-of-service attacks
- Reports from automated scanners with no demonstrated impact
- Missing best-practice headers or settings with no real security impact

## Staying safe

- Only download Vox Launcher from the [official releases page](https://github.com/sylphiras/vox-launcher/releases) or [vox-launcher.com](https://vox-launcher.com).
- Staff and maintainers will **never** ask for your password or account tokens.
- Only install mods and modpacks from sources you trust.

Vox Launcher is not an official Minecraft product and isn't approved by or associated with Mojang Studios or Microsoft.
