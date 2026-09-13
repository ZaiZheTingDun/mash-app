# Security Policy

## Supported versions

Security fixes are provided for the latest stable Mash release. Older releases
may be asked to upgrade before a report is investigated.

## Reporting a vulnerability

Please use GitHub's private **Report a vulnerability** form in this repository's
Security tab. If private vulnerability reporting is not available, contact the
maintainer through a private contact method listed on the maintainer's GitHub
profile.

Do not open a public issue for a vulnerability that has not been fixed. Do not
include credentials, private keys, device identifiers, account identifiers,
private screenshots, or other personal data in a report.

A useful report includes:

- the affected Mash version and operating system;
- the impact and prerequisites;
- minimal reproduction steps or a proof of concept;
- suggested mitigations, if known; and
- whether the issue affects the updater, CV runtime, asset downloads, ADB
  integration, or local data.

Reports are handled on a best-effort basis. The maintainer will coordinate
disclosure after a fix or mitigation is available. This project does not
currently offer a bug bounty.

## Scope

In scope:

- vulnerabilities in Mash source code and release artifacts;
- updater signature or release-integrity failures;
- unsafe archive extraction or runtime installation;
- unintended access to local Mash data, screenshots, or connected devices; and
- command execution that crosses the documented Tauri capability boundary.

Out of scope:

- vulnerabilities in Fate/Grand Order or its services;
- account enforcement or gameplay-policy decisions by a game operator;
- social engineering without a product vulnerability;
- denial of service requiring excessive or destructive traffic; and
- vulnerabilities that exist only in an unsupported Mash release.

Third-party dependency vulnerabilities should normally be reported upstream.
Please also report them privately here when Mash's use of the dependency makes
users directly vulnerable.

## Testing safety

Mash can control an Android device or emulator through ADB. Do not run builds,
scripts, or proof-of-concept code from an untrusted fork while a real device is
connected. Use an isolated emulator and test data, avoid production accounts,
and stop testing if it could modify unrelated user data or external services.

Good-faith research that respects user privacy, avoids destructive activity,
and gives the maintainer reasonable time to address a report is welcome.
